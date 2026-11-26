//! ActiveSupport's `Object#with_options` in code that runs (a boot
//! statement, a library class's body or method): devise's `modules.rb`
//! runs `Devise.with_options model: true do |d| d.add_module … end` as it
//! loads.
//!
//! Rails' `OptionMerger` sends each call made on the block's argument to
//! the receiver, with the options deep-merged into the call's trailing
//! Hash, or appended as one when the call ends in none. Each such call is
//! written so in the block's place, and a nested `with_options` on the
//! argument merges its options into the outer ones. A model's or a
//! controller's class-body `with_options` is their ingest's
//! (`ingest::with_options`, which re-reads the source).
//!
//! Left as written, and reported where analyze reports it: a block that
//! takes no argument (`instance_eval`'s, whose receiverless calls may be
//! the block's own), an argument read other than as a receiver, a
//! receiver that is not a constant, `self` or a local (written once per
//! call), options or a trailing argument that is not a literal Hash with
//! literal keys, and a splat.

use crate::app::App;
use crate::expr::{Expr, ExprNode, Literal};
use crate::ident::Symbol;

pub(super) fn expand(app: &mut App) {
    for stmt in &mut app.boot_program {
        rewrite(stmt);
    }
    for lc in &mut app.library_classes {
        for m in &mut lc.methods {
            rewrite(&mut m.body);
        }
        for e in lc.unknown_calls.iter_mut().chain(lc.class_ivar_initializers.iter_mut()) {
            rewrite(e);
        }
    }
}

fn rewrite(e: &mut Expr) {
    if let Some(statements) = expanded(e) {
        *e = match <[Expr; 1]>::try_from(statements) {
            Ok([only]) => only,
            Err(many) => Expr::new(e.span, ExprNode::Seq { exprs: many }),
        };
    }
    e.node.for_each_child_mut(&mut rewrite);
}

fn expanded(e: &Expr) -> Option<Vec<Expr>> {
    let ExprNode::Send { recv: Some(recv), method, args, block: Some(block), .. } = &*e.node else { return None };
    if method.as_str() != "with_options" || !matches!(&*recv.node, ExprNode::Const { .. } | ExprNode::SelfRef | ExprNode::Var { .. }) {
        return None;
    }
    let [options] = args.as_slice() else { return None };
    literal_keys(options)?;
    let ExprNode::Lambda { params, rest_param: None, extra_params, block_param: None, body, .. } = &*block.node else { return None };
    let ([arg], true) = (params.as_slice(), extra_params.is_empty()) else { return None };
    let statements: Vec<&Expr> = match &*body.node {
        ExprNode::Seq { exprs } => exprs.iter().collect(),
        _ => vec![body],
    };
    statements.into_iter().map(|s| merged(s, arg, recv, options)).collect()
}

/// `stmt` with each call on `arg` sent to `recv` with `options` merged in; `None` when `arg` is read otherwise.
fn merged(stmt: &Expr, arg: &Symbol, recv: &Expr, options: &Expr) -> Option<Expr> {
    let mut out = stmt.clone();
    let mut ok = true;
    redirect(&mut out, arg, recv, options, &mut ok);
    ok.then_some(out)
}

fn redirect(e: &mut Expr, arg: &Symbol, recv: &Expr, options: &Expr, ok: &mut bool) {
    if !*ok {
        return;
    }
    if let ExprNode::Send { recv: Some(r), method, args, .. } = &mut *e.node
        && matches!(&*r.node, ExprNode::Var { name, .. } if name == arg)
    {
        *r = recv.clone();
        if args.iter().any(|a| matches!(&*a.node, ExprNode::Splat { .. })) {
            *ok = false;
            return;
        }
        let nested = method.as_str() == "with_options";
        match args.last_mut() {
            Some(last) if literal_keys(last).is_some() => *last = deep_merge(options, last),
            Some(last) if nested || maybe_hash(last) => {
                *ok = false;
                return;
            }
            _ if nested => {
                *ok = false;
                return;
            }
            _ => args.push(as_kwargs(options)),
        }
    } else if matches!(&*e.node, ExprNode::Var { name, .. } if name == arg) {
        *ok = false;
        return;
    }
    e.node.for_each_child_mut(&mut |c| redirect(c, arg, recv, options, ok));
}

/// Not a literal that is no Hash: a local or a call could hold one, which Rails would merge into.
fn maybe_hash(e: &Expr) -> bool {
    !matches!(&*e.node, ExprNode::Lit { .. } | ExprNode::Array { .. } | ExprNode::StringInterp { .. } | ExprNode::Lambda { .. })
}

fn literal_keys(e: &Expr) -> Option<()> {
    let ExprNode::Hash { entries, .. } = &*e.node else { return None };
    entries.iter().all(|(k, _)| key_of(k).is_some()).then_some(())
}

fn key_of(e: &Expr) -> Option<String> {
    match &*e.node {
        ExprNode::Lit { value: Literal::Sym { value } } => Some(format!(":{value}")),
        ExprNode::Lit { value: Literal::Str { value } } => Some(format!("\"{value}")),
        _ => None,
    }
}

fn as_kwargs(options: &Expr) -> Expr {
    let mut out = options.clone();
    if let ExprNode::Hash { kwargs, .. } = &mut *out.node {
        *kwargs = true;
    }
    out
}

/// AS `Hash#deep_merge`: `call`'s entries over `options`', nested literal Hashes merged in turn.
fn deep_merge(options: &Expr, call: &Expr) -> Expr {
    let (ExprNode::Hash { entries: base, .. }, ExprNode::Hash { entries: over, kwargs }) = (&*options.node, &*call.node) else {
        return call.clone();
    };
    let mut entries = base.clone();
    for (k, v) in over {
        match entries.iter_mut().find(|(ek, _)| key_of(ek) == key_of(k)) {
            Some((_, existing)) if literal_keys(existing).is_some() && literal_keys(v).is_some() => *existing = deep_merge(existing, v),
            Some((_, existing)) => *existing = v.clone(),
            None => entries.push((k.clone(), v.clone())),
        }
    }
    Expr::new(call.span, ExprNode::Hash { entries, kwargs: *kwargs })
}
