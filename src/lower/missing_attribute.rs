//! A column a partial `select` left out raises
//! `ActiveModel::MissingAttributeError` when its generated reader reads
//! it, as Rails' does (activerecord 8.1.4, attribute_methods/read.rb:
//! the generated reader reads `_read_attribute(name) { |n|
//! missing_attribute(n, caller) }`). `Post.select(:id).take!.title`
//! raises "missing attribute 'title' for Post" rather than answering nil.
//!
//! A row hydrated from an explicit `select` notes the columns it did not
//! carry (`instantiate` calls `Base#_note_unloaded`, connection.rb). Each
//! generated column reader then starts with
//!
//!   _missing_attribute(:title) if @__unloaded_columns
//!
//! which raises when the record left that column out. A record loaded
//! without an explicit `select` notes nothing, so the common read is one
//! ivar test. The guard goes on the
//! GENERATED reader only: where the model defines the column's reader
//! itself, the generated one is `_read_<col>_raw` (`push_user_methods`),
//! which the model's `super` reaches, so a model method that reads the
//! column raises through `super` and one that does not still answers.
//! The generated reader is defined on the model class itself, so it
//! answers before a module the model includes (Rails puts it below them);
//! a module reader that answers `super` (`def title = super`) reaches the
//! same raise in Rails. The primary key is not guarded: Rails' `id` reads
//! it without the raising block (PrimaryKey#id).
//!
//! Assigning a left-out column loads it, as Rails' writer replaces the
//! uninitialized attribute: each generated column writer (`_write_<col>_raw`
//! where the model defines the writer itself) and each column arm of the
//! generated `[]=` starts with `_note_loaded(:title) if @__unloaded_columns`,
//! which drops the column from the row's list.
//!
//! The generated `serializable_hash` reads every column's reader, so it
//! would raise too. Rails' writes the
//! record's `attribute_names` instead (activemodel serialization.rb,
//! `serializable_attributes(attribute_names)`): the loaded columns and the
//! primary key, which `_note_unloaded` never notes. So for a record that
//! noted unloaded columns it builds the Hash without them, and a whole row
//! answers the literal Hash as before. `to_json` / `render json:` reach the
//! column writers `lower::as_json_poro` builds (`as_json_str`, and one per
//! `only:`/`except:`/`methods:` list), which skip them the same way;
//! `as_json` skips them in the runtime (`_as_json_only`, and the CRuby
//! overlay's `as_json`).
//!
//! Ruby-family only, applied at emit after the passes that rewrite reader
//! bodies by shape (`apply_datetime_lowering`, `apply_boolean_lowering`):
//! the strict targets have no partial-select rows (`_note_unloaded` is
//! empty there).

use crate::app::App;
use crate::dialect::{LibraryClass, MethodReceiver};
use crate::expr::{Expr, ExprNode, LValue, Literal};
use crate::ident::{Symbol, VarId};
use crate::span::Span;

const UNLOADED_IVAR: &str = "__unloaded_columns";

pub fn apply(lcs: &mut [LibraryClass], app: &App) {
    for model in &app.models {
        let Some(table) = app.schema.tables.get(&model.table.0) else { continue };
        let Some(lc) = lcs.iter_mut().find(|lc| lc.name == model.name) else { continue };
        let primary_key = model.primary_key.clone().unwrap_or_else(|| Symbol::from("id"));
        for col in table.columns.iter().filter(|c| c.name != primary_key) {
            let instance = |name: String| {
                let name = Symbol::from(name);
                lc.methods.iter().position(|m| m.receiver == MethodReceiver::Instance && m.name == name)
            };
            let col_name = col.name.as_str();
            let reader = instance(format!("_read_{col_name}_raw")).or_else(|| instance(col_name.to_string()));
            let writer = instance(format!("_write_{col_name}_raw")).or_else(|| instance(format!("{col_name}=")));
            if let Some(at) = reader {
                prepend(&mut lc.methods[at].body, if_unloaded("_missing_attribute", &col.name));
            }
            if let Some(at) = writer {
                prepend(&mut lc.methods[at].body, if_unloaded("_note_loaded", &col.name));
            }
        }
        // `record[:col] = value` loads the column as its writer does.
        if let Some(m) = lc.methods.iter_mut().find(|m| m.receiver == MethodReceiver::Instance && m.name.as_str() == "[]=")
            && let ExprNode::Case { arms, .. } = &mut *m.body.node
        {
            for arm in arms {
                let crate::expr::Pattern::Lit { value: Literal::Sym { value: col } } = &arm.pattern else { continue };
                if col != &primary_key {
                    let note = if_unloaded("_note_loaded", col);
                    prepend(&mut arm.body, note);
                }
            }
        }
        if let Some(m) = lc
            .methods
            .iter_mut()
            .find(|m| m.receiver == MethodReceiver::Instance && m.name.as_str() == "serializable_hash")
        {
            loaded_serializable_hash(&mut m.body);
        }
        // A model's own `as_json` writer serializes what that method reads,
        // which raises in Rails as well when it reads a left-out column.
        let own_as_json = model.body.iter().any(|item| {
            matches!(item, crate::dialect::ModelBodyItem::Method { method, .. }
                if method.name.as_str() == "as_json" && method.receiver == MethodReceiver::Instance)
        });
        if !own_as_json {
            let columns: Vec<&str> = table.columns.iter().map(|c| c.name.as_str()).collect();
            for m in lc.methods.iter_mut().filter(|m| {
                m.receiver == MethodReceiver::Instance
                    && (m.name.as_str() == WRITER || m.name.as_str().starts_with(&format!("{WRITER}__")))
            }) {
                loaded_json_writer(&mut m.body, &columns);
            }
        }
    }
}

const WRITER: &str = crate::lower::JSON_WRITER_METHOD;

fn unloaded() -> Expr {
    Expr::new(Span::synthetic(), ExprNode::Ivar { name: Symbol::from(UNLOADED_IVAR) })
}

/// `<stmt> unless @__unloaded_columns.include?(:<col>)`.
fn unless_left_out(col: &str, stmt: Expr) -> Expr {
    let s = Span::synthetic();
    let left_out = Expr::new(
        s,
        ExprNode::Send {
            recv: Some(unloaded()),
            method: Symbol::from("include?"),
            args: vec![Expr::new(s, ExprNode::Lit { value: Literal::Sym { value: Symbol::from(col) } })],
            block: None,
            parenthesized: true,
        },
    );
    Expr::new(
        s,
        ExprNode::If { cond: left_out, then_branch: Expr::new(s, ExprNode::Lit { value: Literal::Nil }), else_branch: stmt },
    )
}

/// `if @__unloaded_columns then <partial> else <whole> end`, as `body`.
fn when_unloaded(body: &mut Expr, partial: Vec<Expr>) {
    let s = Span::synthetic();
    let whole = std::mem::replace(body, Expr::new(s, ExprNode::Seq { exprs: vec![] }));
    let ty = whole.ty.clone();
    *body = Expr::new(
        s,
        ExprNode::If { cond: unloaded(), then_branch: Expr::new(s, ExprNode::Seq { exprs: partial }), else_branch: whole },
    );
    body.ty = ty;
}

/// The column writer `as_json_writer::writer_body` builds,
///
///   io = String.new
///   io << "{"
///   io << "\"id\":"
///   io << JsonBuilder.encode_value(self.id)
///   io << ",\"title\":"
///   io << JsonBuilder.encode_value(self.title)
///   io << "}"
///   io
///
/// gains a partial-row arm that writes each column's pair unless the row
/// left it out, its comma kept in `__sep`:
///
///   unless @__unloaded_columns.include?(:title)
///     io << __sep
///     io << "\"title\":"
///     io << JsonBuilder.encode_value(self.title)
///     __sep = ","
///   end
///
/// A pair that is no column (`methods:`) is written unconditionally. Any
/// other shape (a conditional pair) is left as it is.
fn loaded_json_writer(body: &mut Expr, columns: &[&str]) {
    let ExprNode::Seq { exprs } = &*body.node else { return };
    let n = exprs.len();
    if n < 6 || n % 2 != 0 {
        return;
    }
    let s = Span::synthetic();
    let sep = Symbol::from("__sep");
    let set_sep = |text: &str| {
        let value = crate::lower::typing::lit_str(text.to_string());
        crate::lower::typing::with_ty(
            Expr::new(s, ExprNode::Assign { target: LValue::Var { id: VarId(0), name: sep.clone() }, value }),
            crate::ty::Ty::Str,
        )
    };
    // `io << <arg>`, from a key statement of the writer.
    let append = |template: &Expr, arg: Expr| {
        let mut e = template.clone();
        if let ExprNode::Send { args, .. } = &mut *e.node {
            *args = vec![arg];
        }
        e
    };
    let mut partial = vec![exprs[0].clone(), exprs[1].clone(), set_sep("")];
    for pair in exprs[2..n - 2].chunks(2) {
        let ExprNode::Send { method, args, .. } = &*pair[0].node else { return };
        let [arg] = args.as_slice() else { return };
        let ExprNode::Lit { value: Literal::Str { value: text } } = &*arg.node else { return };
        if method.as_str() != "<<" {
            return;
        }
        let Some(col) = text.trim_start_matches(',').strip_prefix('"').and_then(|k| k.strip_suffix("\":")) else {
            return;
        };
        let key = crate::lower::typing::lit_str(format!("\"{col}\":"));
        let sep_read =
            crate::lower::typing::with_ty(Expr::new(s, ExprNode::Var { id: VarId(0), name: sep.clone() }), crate::ty::Ty::Str);
        let group = Expr::new(
            s,
            ExprNode::Seq {
                exprs: vec![append(&pair[0], sep_read), append(&pair[0], key), pair[1].clone(), set_sep(",")],
            },
        );
        partial.push(if columns.contains(&col) { unless_left_out(col, group) } else { group });
    }
    partial.extend(exprs[n - 2..].iter().cloned());
    when_unloaded(body, partial);
}

/// `{ "id" => id, "title" => title }` becomes
///
///   if @__unloaded_columns
///     __hash = {}
///     __hash["id"] = id unless @__unloaded_columns.include?(:id)
///     __hash["title"] = title unless @__unloaded_columns.include?(:title)
///     __hash
///   else
///     { "id" => id, "title" => title }
///   end
fn loaded_serializable_hash(body: &mut Expr) {
    let ExprNode::Hash { entries, kwargs: false } = &*body.node else { return };
    let s = Span::synthetic();
    let hash = Symbol::from("__hash");
    let var = || Expr::new(s, ExprNode::Var { id: VarId(0), name: hash.clone() });
    let mut stmts = vec![Expr::new(
        s,
        ExprNode::Assign {
            target: LValue::Var { id: VarId(0), name: hash.clone() },
            value: Expr::new(s, ExprNode::Hash { entries: vec![], kwargs: false }),
        },
    )];
    for (key, value) in entries {
        let ExprNode::Lit { value: Literal::Str { value: name } } = &*key.node else { return };
        let store = Expr::new(
            s,
            ExprNode::Assign { target: LValue::Index { recv: var(), index: key.clone() }, value: value.clone() },
        );
        stmts.push(unless_left_out(name, store));
    }
    stmts.push(var());
    when_unloaded(body, stmts);
}

/// `body` with `stmt` run first, its value still the body's.
fn prepend(body: &mut Expr, stmt: Expr) {
    match &mut *body.node {
        ExprNode::Seq { exprs } => exprs.insert(0, stmt),
        _ => {
            let rest = std::mem::replace(body, Expr::new(Span::synthetic(), ExprNode::Seq { exprs: vec![] }));
            let ty = rest.ty.clone();
            *body = Expr::new(rest.span, ExprNode::Seq { exprs: vec![stmt, rest] });
            body.ty = ty;
        }
    }
}

/// `<method>(:<col>) if @__unloaded_columns`: the reader's
/// `_missing_attribute` guard, or the writer's `_note_loaded`.
fn if_unloaded(method: &str, col: &Symbol) -> Expr {
    let s = Span::synthetic();
    Expr::new(
        s,
        ExprNode::If {
            cond: unloaded(),
            then_branch: Expr::new(
                s,
                ExprNode::Send {
                    recv: None,
                    method: Symbol::from(method),
                    args: vec![Expr::new(s, ExprNode::Lit { value: Literal::Sym { value: col.clone() } })],
                    block: None,
                    parenthesized: true,
                },
            ),
            else_branch: Expr::new(s, ExprNode::Lit { value: Literal::Nil }),
        },
    )
}
