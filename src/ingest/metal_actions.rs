//! A class under `ActionController::Metal` reached through `action(:name)` (Devise's FailureApp:
//! `@respond ||= action(:respond)`): Rails' `process(name)` sends the action by name, which an emitted program
//! cannot, so the names the class hands `action` are a `process_action` of its own, as a routed controller's are.
//! A single `around_action do |controller, action| … end` block becomes that dispatch's body, its `action`
//! the dispatch and its `controller` self, as AbstractController::Callbacks runs it around `process_action`.
//! Any other callback the class declares is reported unsupported: the dispatch would run without it.

use crate::App;
use crate::dialect::MethodReceiver;
use crate::expr::{Expr, ExprNode, Literal};

pub(super) fn synthesize(app: &mut App) {
    let parents: crate::det::HashMap<String, String> = app
        .library_classes
        .iter()
        .filter_map(|c| c.parent.as_ref().map(|p| (c.name.0.as_str().to_string(), p.0.as_str().trim_start_matches("::").to_string())))
        .collect();
    let under_metal = |name: &str| {
        let mut cur = name.to_string();
        for _ in 0..32 {
            match parents.get(&cur) {
                Some(p) if p == "ActionController::Metal" => return true,
                Some(p) => cur = p.clone(),
                None => return false,
            }
        }
        false
    };
    let mut made = Vec::new();
    for (at, class) in app.library_classes.iter().enumerate() {
        if !under_metal(class.name.0.as_str()) || class.methods.iter().any(|m| m.receiver == MethodReceiver::Instance && m.name.as_str() == "process_action") {
            continue;
        }
        let mut names: Vec<String> = Vec::new();
        for m in class.methods.iter().filter(|m| m.receiver == MethodReceiver::Class) {
            collect_action_names(&m.body, &mut names);
        }
        let defined = |n: &str| class.methods.iter().any(|m| m.receiver == MethodReceiver::Instance && m.name.as_str() == n);
        names.retain(|n| defined(n));
        if names.is_empty() {
            continue;
        }
        let arms: String = names.iter().map(|n| format!("    when :{n} then {n}\n")).collect();
        let src = format!("class {}\n  def process_action(action_name)\n    case action_name\n{arms}    end\n  end\nend\n", class.name.0.as_str());
        if let Ok(classes) = super::prism::scope(|| super::ingest_library_classes(src.as_bytes(), "<metal actions>")).0 {
            let mut methods: Vec<_> = classes.into_iter().flat_map(|c| c.methods).collect();
            let mut wrapped_around = false;
            if let Some((controller, action, body)) = around_block(&class.unknown_calls)
                && let Some(m) = methods.first_mut()
                && let Some(wrapped) = around(body, controller, action, &m.body)
            {
                m.body = wrapped;
                wrapped_around = true;
            }
            made.push((at, methods, wrapped_around));
        }
    }
    for (at, methods, wrapped_around) in made {
        let lc = &mut app.library_classes[at];
        if wrapped_around {
            lc.unknown_calls.retain(|c| around_block(std::slice::from_ref(c)).is_none());
        }
        // Not run around the dispatch, and not dropped in silence: a callback the dispatch leaves out (a `before_action`'s check) is what Rails would have run first.
        for call in &lc.unknown_calls {
            if let ExprNode::Send { method, .. } = &*call.node
                && super::library_class::is_controller_callback_macro(method.as_str())
            {
                super::prism::push(crate::diagnostic::Diagnostic::unsupported(
                    call.span,
                    None,
                    "callback",
                    format!(
                        "`{}` on `{}` is not run around the actions `action(:name)` dispatches: only a single `around_action do |controller, action| … end` block is",
                        method.as_str(),
                        lc.name.0.as_str()
                    ),
                ));
            }
        }
        lc.methods.extend(methods);
    }
}

// More than one block nests in declaration order with its conditions, which this does not order, so only one is taken.
fn around_block(calls: &[Expr]) -> Option<(&str, &str, &Expr)> {
    let mut found = calls.iter().filter_map(|c| match &*c.node {
        ExprNode::Send { recv: None, method, args, block: Some(b), .. } if method.as_str() == "around_action" && args.is_empty() => match &*b.node {
            ExprNode::Lambda { params, rest_param: None, extra_params, block_param: None, body, .. } if params.len() == 2 && extra_params.is_empty() => {
                Some((params[0].as_str(), params[1].as_str(), body))
            }
            _ => None,
        },
        _ => None,
    });
    let one = found.next()?;
    found.next().is_none().then_some(one)
}

fn around(body: &Expr, controller: &str, action: &str, dispatch: &Expr) -> Option<Expr> {
    let mut out = body.clone();
    let mut ok = true;
    replace(&mut out, controller, action, dispatch, &mut ok);
    ok.then_some(out)
}

fn is_var(e: &Expr, name: &str) -> bool {
    matches!(&*e.node, ExprNode::Var { name: n, .. } if n.as_str() == name)
}

fn replace(e: &mut Expr, controller: &str, action: &str, dispatch: &Expr, ok: &mut bool) {
    if is_var(e, controller) {
        *e = Expr::new(e.span, ExprNode::SelfRef);
        return;
    }
    if let ExprNode::Send { recv: Some(r), method, args, block: None, .. } = &*e.node
        && method.as_str() == "call"
        && args.is_empty()
        && is_var(r, action)
    {
        *e = dispatch.clone();
        return;
    }
    if let ExprNode::Send { block: Some(b), .. } = &mut *e.node
        && is_var(b, action)
    {
        *b = Expr::new(b.span, ExprNode::Lambda {
            params: Vec::new(),
            rest_param: None,
            extra_params: Vec::new(),
            block_param: None,
            body: dispatch.clone(),
            block_style: Default::default(),
        });
    }
    if is_var(e, action) {
        *ok = false;
        return;
    }
    e.node.for_each_child_mut(&mut |c| replace(c, controller, action, dispatch, ok));
}

fn collect_action_names(e: &Expr, out: &mut Vec<String>) {
    if let ExprNode::Send { recv, method, args, block: None, .. } = &*e.node
        && method.as_str() == "action"
        && recv.as_ref().is_none_or(|r| matches!(&*r.node, ExprNode::SelfRef))
        && let [arg] = args.as_slice()
        && let ExprNode::Lit { value: Literal::Sym { value } } = &*arg.node
        && !out.iter().any(|n| n == value.as_str())
    {
        out.push(value.as_str().to_string());
    }
    e.node.for_each_child(&mut |c| collect_action_names(c, out));
}
