//! A controller's `content_security_policy(only: :show) do |policy| … end` (lobsters' stories): Rails' helper is a
//! `before_action` that clones the request's policy, runs the block on it and stores it back, and a lambda source
//! (`-> { @story&.can_have_images? ? "https:" : "" }`) runs in the controller as the header is built. This becomes a
//! private method doing the first, with each lambda held as `ContentSecurityPolicy.dynamic_source(i)`, a
//! `before_action` running it with the block's `only:` / `except:`, and `content_security_policy_source(i)` answering
//! the lambdas' bodies when the header is written. A block this cannot read (a directive with no source, a computed
//! source, other code, `content_security_policy false`) stays where it was and is reported.

use crate::dialect::{Controller, ControllerBodyItem, Filter, FilterKind};
use crate::expr::{Expr, ExprNode, Literal};
use crate::Symbol;

pub(super) fn expand(app: &mut crate::App) {
    let mut next_source = 0usize;
    let mut next_method = 0usize;
    let mut any = false;
    for controller in &mut app.controllers {
        any |= expand_controller(controller, &mut next_source, &mut next_method);
    }
    // Rails' middleware writes the header on every response of a request with a policy. Every controller is marked,
    // not only those a block names: the header write is a no-op where the request has none.
    let app_policy = app.rails_application.as_ref().is_some_and(|lc| lc.methods.iter().any(|m| m.name.as_str() == "content_security_policy"));
    if any || app_policy {
        for controller in &mut app.controllers {
            controller.csp_header = true;
        }
    }
}

struct Block {
    at: usize,
    method: String,
    only: Vec<Symbol>,
    except: Vec<Symbol>,
    lines: Vec<String>,
}

/// Whether the controller sets a policy of its own.
fn expand_controller(controller: &mut Controller, next_source: &mut usize, next_method: &mut usize) -> bool {
    let mut blocks: Vec<Block> = Vec::new();
    let mut arms: Vec<(usize, String)> = Vec::new();
    for (at, item) in controller.body.iter().enumerate() {
        let ControllerBodyItem::Unknown { expr, .. } = item else { continue };
        let mut local_arms = Vec::new();
        let mut source_index = *next_source;
        let Some((only, except, lines)) = read(expr, &mut source_index, &mut local_arms) else { continue };
        *next_source = source_index;
        arms.extend(local_arms);
        blocks.push(Block { at, method: format!("content_security_policy_{}", *next_method), only, except, lines });
        *next_method += 1;
    }
    if blocks.is_empty() {
        return false;
    }
    let mut src = format!("class {} < ApplicationController\n  private\n", controller.name.0.as_str());
    for block in &blocks {
        src.push_str(&format!(
            "  def {}\n    policy = request.content_security_policy\n    if policy.nil?\n      policy = ActionDispatch::ContentSecurityPolicy.new\n      request.content_security_policy = policy\n    end\n",
            block.method
        ));
        for line in &block.lines {
            src.push_str(&format!("    {line}\n"));
        }
        src.push_str("    nil\n  end\n");
    }
    if !arms.is_empty() {
        src.push_str("  def content_security_policy_source(index)\n    case index\n");
        for (i, body) in &arms {
            src.push_str(&format!("    when {i}\n      ({body}).to_s\n"));
        }
        src.push_str("    else\n      super\n    end\n  end\n");
    }
    src.push_str("end\n");
    let (result, diags) = crate::ingest::prism::scope(|| crate::ingest::controller::ingest_controller(src.as_bytes(), "<content_security_policy>"));
    let parsed = match (result, diags.is_empty()) {
        (Ok(Some(c)), true) => c,
        (Ok(_), false) => {
            super::survey::record_synthesis_failure("<content_security_policy>", &format!("content_security_policy of `{}`", controller.name.0.as_str()), &diags);
            return false;
        }
        (Ok(None), true) => return false,
        (Err(err), _) => {
            super::survey::record(&err);
            return false;
        }
    };
    for block in &blocks {
        if let ControllerBodyItem::Unknown { leading_comments, leading_blank_line, .. } = &mut controller.body[block.at] {
            let item = ControllerBodyItem::Filter {
                filter: filter(&block.method, &block.only, &block.except),
                leading_comments: std::mem::take(leading_comments),
                leading_blank_line: *leading_blank_line,
            };
            controller.body[block.at] = item;
        }
    }
    if !controller.body.iter().any(|item| matches!(item, ControllerBodyItem::PrivateMarker { .. })) {
        controller.body.push(ControllerBodyItem::PrivateMarker { leading_comments: Vec::new(), leading_blank_line: true });
    }
    for item in parsed.body {
        if let ControllerBodyItem::Action { action, .. } = item {
            controller.body.push(ControllerBodyItem::Action { action, leading_comments: Vec::new(), leading_blank_line: true });
        }
    }
    true
}

/// The call's `only:` / `except:` and one `policy.set_directive(…)` line per statement, each lambda source given the
/// next index and its body's source.
fn read(expr: &Expr, next: &mut usize, arms: &mut Vec<(usize, String)>) -> Option<(Vec<Symbol>, Vec<Symbol>, Vec<String>)> {
    let ExprNode::Send { recv: None, method, args, block: Some(block), .. } = &*expr.node else { return None };
    if method.as_str() != "content_security_policy" {
        return None;
    }
    let (mut only, mut except) = (Vec::new(), Vec::new());
    match args.as_slice() {
        [] => {}
        [options] => {
            let ExprNode::Hash { entries, .. } = &*options.node else { return None };
            for (k, v) in entries {
                let ExprNode::Lit { value: Literal::Sym { value: key } } = &*k.node else { return None };
                let names = symbols(v)?;
                match key.as_str() {
                    "only" => only = names,
                    "except" => except = names,
                    _ => return None,
                }
            }
        }
        _ => return None,
    }
    let ExprNode::Lambda { params, rest_param: None, block_param: None, extra_params, body, .. } = &*block.node else { return None };
    let [param] = params.as_slice() else { return None };
    if !extra_params.is_empty() {
        return None;
    }
    let stmts: Vec<&Expr> = match &*body.node {
        ExprNode::Seq { exprs } => exprs.iter().collect(),
        _ => vec![body],
    };
    let mut lines = Vec::new();
    for stmt in stmts {
        let ExprNode::Send { recv: Some(recv), method: directive, args, block: None, .. } = &*stmt.node else { return None };
        if !matches!(&*recv.node, ExprNode::Var { name, .. } if name == param) || !super::app::CSP_DIRECTIVES.contains(&directive.as_str()) || args.is_empty() {
            return None;
        }
        let mut sources = Vec::new();
        for arg in args {
            match &*arg.node {
                ExprNode::Lit { value: Literal::Sym { value } } => sources.push(format!("{:?}", super::app::csp_source_mapping(value.as_str())?)),
                ExprNode::Lit { value: Literal::Str { value } } => sources.push(format!("{value:?}")),
                ExprNode::Lambda { params, rest_param: None, block_param: None, extra_params, body, .. } if params.is_empty() && extra_params.is_empty() => {
                    arms.push((*next, source_text(body)?));
                    sources.push(format!("ActionDispatch::ContentSecurityPolicy.dynamic_source({})", *next));
                    *next += 1;
                }
                _ => return None,
            }
        }
        lines.push(format!("policy.set_directive({:?}, [{}])", directive.as_str().replace('_', "-"), sources.join(", ")));
    }
    Some((only, except, lines))
}

fn symbols(v: &Expr) -> Option<Vec<Symbol>> {
    match &*v.node {
        ExprNode::Lit { value: Literal::Sym { value } } => Some(vec![value.clone()]),
        ExprNode::Array { elements, .. } => elements
            .iter()
            .map(|e| match &*e.node {
                ExprNode::Lit { value: Literal::Sym { value } } => Some(value.clone()),
                _ => None,
            })
            .collect(),
        _ => None,
    }
}

fn source_text(body: &Expr) -> Option<String> {
    if body.span.is_synthetic() {
        return None;
    }
    let path = super::sources::path_of(body.span.file)?;
    let text = super::sources::text_of(&path)?;
    text.get(body.span.start as usize..body.span.end as usize).map(str::to_string)
}

fn filter(method: &str, only: &[Symbol], except: &[Symbol]) -> Filter {
    Filter {
        target_span: crate::span::Span::synthetic(),
        kind: FilterKind::Before,
        target: Symbol::from(method),
        from_concern: None,
        only: only.to_vec(),
        except: except.to_vec(),
        only_style: Default::default(),
        except_style: Default::default(),
        if_cond: None,
        unless_cond: None,
        if_cond_expr: None,
        unless_cond_expr: None,
        unread_guard: false,
        unread_options: false,
        only_present: !only.is_empty(),
        block: None,
        prepend: false,
    }
}
