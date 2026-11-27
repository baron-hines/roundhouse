//! A call whose argument is Ruby's own NameError: the call never runs.
//!
//! `error.is_a?(::RedisClient::Cluster::InitialSetupError)` behind a
//! `defined?` (redis without redis-clustering) reads a constant nothing
//! defines, which the analyzer writes as `raise NameError.new(…)`
//! (`GUARDED_NAME_ERROR`, `SINGLETON_NAME_ERROR`). Ruby evaluates the
//! receiver, then the arguments in order, and raises at that one. Where
//! the receiver and the arguments ahead of it are plain reads, the call is
//! that raise, which a target types as it types any raise rather than as
//! an `is_a?` of a non-class.

use crate::expr::{Expr, ExprNode};

pub(crate) fn apply(app: &mut crate::App) {
    super::for_each_emit_body(app, &mut |body| rewrite(body));
}

fn rewrite(e: &mut Expr) {
    e.node.for_each_child_mut(&mut rewrite);
    let ExprNode::Send { recv, args, block: None, .. } = &*e.node else { return };
    if !recv.as_ref().is_none_or(plain_read) {
        return;
    }
    let Some(at) = args.iter().position(name_error) else { return };
    if !args[..at].iter().all(plain_read) {
        return;
    }
    let raised = args[at].clone();
    *e = Expr { span: e.span, ..raised };
}

fn name_error(e: &Expr) -> bool {
    matches!(&*e.node, ExprNode::Raise { .. })
        && e.decisions & (crate::expr::GUARDED_NAME_ERROR | crate::expr::SINGLETON_NAME_ERROR) != 0
}

fn plain_read(e: &Expr) -> bool {
    matches!(&*e.node, ExprNode::Var { .. } | ExprNode::Ivar { .. } | ExprNode::SelfRef | ExprNode::Lit { .. } | ExprNode::Const { .. })
}
