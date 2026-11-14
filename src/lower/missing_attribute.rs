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
//! Ruby-family only, applied at emit after the passes that rewrite reader
//! bodies by shape (`apply_datetime_lowering`, `apply_boolean_lowering`):
//! the strict targets have no partial-select rows (`_note_unloaded` is
//! empty there).

use crate::app::App;
use crate::dialect::{LibraryClass, MethodReceiver};
use crate::expr::{Expr, ExprNode, Literal};
use crate::ident::Symbol;
use crate::span::Span;

const UNLOADED_IVAR: &str = "__unloaded_columns";

pub fn apply(lcs: &mut [LibraryClass], app: &App) {
    for model in &app.models {
        let Some(table) = app.schema.tables.get(&model.table.0) else { continue };
        let Some(lc) = lcs.iter_mut().find(|lc| lc.name == model.name) else { continue };
        let primary_key = model.primary_key.clone().unwrap_or_else(|| Symbol::from("id"));
        for col in table.columns.iter().filter(|c| c.name != primary_key) {
            let renamed = Symbol::from(format!("_read_{}_raw", col.name.as_str()));
            let instance = |name: &Symbol| {
                lc.methods.iter().position(|m| m.receiver == MethodReceiver::Instance && &m.name == name)
            };
            let Some(at) = instance(&renamed).or_else(|| instance(&col.name)) else { continue };
            let body = &mut lc.methods[at].body;
            let guard = guard(&col.name);
            match &mut *body.node {
                ExprNode::Seq { exprs } => exprs.insert(0, guard),
                _ => {
                    let read = std::mem::replace(body, Expr::new(Span::synthetic(), ExprNode::Seq { exprs: vec![] }));
                    let ty = read.ty.clone();
                    *body = Expr::new(read.span, ExprNode::Seq { exprs: vec![guard, read] });
                    body.ty = ty;
                }
            }
        }
    }
}

/// `_missing_attribute(:<col>) if @__unloaded_columns`.
fn guard(col: &Symbol) -> Expr {
    let s = Span::synthetic();
    Expr::new(
        s,
        ExprNode::If {
            cond: Expr::new(s, ExprNode::Ivar { name: Symbol::from(UNLOADED_IVAR) }),
            then_branch: Expr::new(
                s,
                ExprNode::Send {
                    recv: None,
                    method: Symbol::from("_missing_attribute"),
                    args: vec![Expr::new(s, ExprNode::Lit { value: Literal::Sym { value: col.clone() } })],
                    block: None,
                    parenthesized: true,
                },
            ),
            else_branch: Expr::new(s, ExprNode::Lit { value: Literal::Nil }),
        },
    )
}
