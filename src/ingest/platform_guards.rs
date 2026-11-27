//! A load-time branch that tests the platform for its constants before reading them: redis-client's
//! `if %i[SOL_TCP SOL_SOCKET TCP_KEEPIDLE TCP_KEEPINTVL TCP_KEEPCNT].all? { |c| Socket.const_defined? c } # Linux`
//! defines `enable_socket_keep_alive` with `Socket::SOL_TCP` and `Socket::TCP_KEEPIDLE`, which macOS lacks, and a
//! later branch for macOS. Ruby takes the branch on the platform the program runs on, so a read there of a name the
//! branch tests is of a constant that exists. It is marked for the analyzer
//! ([`crate::expr::PLATFORM_GUARDED_CONST`]); a read the branch does not test stays the gap it was.

use crate::App;
use crate::dialect::{ConditionalDefinitions, LibraryClass};
use crate::expr::{Expr, ExprNode, Literal};

pub(super) fn mark(app: &mut App) {
    for lc in &mut app.library_classes {
        let conditionals = std::mem::take(&mut lc.conditional_definitions);
        for conditional in &conditionals {
            mark_conditional(lc, conditional, &[]);
        }
        lc.conditional_definitions = conditionals;
    }
}

fn mark_conditional(lc: &mut LibraryClass, conditional: &ConditionalDefinitions, outer: &[String]) {
    for (i, arm) in conditional.arms.iter().enumerate() {
        let mut tested = outer.to_vec();
        if !(conditional.negated && i == 0) {
            tested.extend(arm.condition.as_ref().map(socket_names_tested).unwrap_or_default());
        }
        if tested.is_empty() {
            continue;
        }
        for member in &arm.methods {
            for method in lc.methods.iter_mut().filter(|m| member.is(m)) {
                mark_reads(&mut method.body, &tested);
            }
        }
        for nested in &arm.nested {
            mark_conditional(lc, nested, &tested);
        }
    }
}

/// The names a condition is true only when `Socket` defines: `Socket.const_defined?(:N)`, `%i[…].all? { |c|
/// Socket.const_defined? c }`, and either side of `&&`.
fn socket_names_tested(condition: &Expr) -> Vec<String> {
    match &*condition.node {
        ExprNode::BoolOp { op: crate::expr::BoolOpKind::And, left, right, .. } => {
            let mut out = socket_names_tested(left);
            out.extend(socket_names_tested(right));
            out
        }
        ExprNode::Send { recv: Some(recv), method, args, block: None, .. } if is_socket_const_defined(recv, method.as_str()) => {
            args.iter().take(1).filter_map(name_literal).collect()
        }
        ExprNode::Send { recv: Some(recv), method, args, block: Some(block), .. } if method.as_str() == "all?" && args.is_empty() => {
            let (ExprNode::Array { elements, .. }, ExprNode::Lambda { params, body, .. }) = (&*recv.node, &*block.node) else {
                return Vec::new();
            };
            let [param] = params.as_slice() else { return Vec::new() };
            let tests_param = matches!(&*body.node, ExprNode::Send { recv: Some(r), method, args, block: None, .. }
                if is_socket_const_defined(r, method.as_str())
                    && matches!(args.as_slice(), [a] if matches!(&*a.node, ExprNode::Var { name, .. } if name == param)));
            if !tests_param {
                return Vec::new();
            }
            elements.iter().map(name_literal).collect::<Option<Vec<_>>>().unwrap_or_default()
        }
        _ => Vec::new(),
    }
}

fn is_socket_const_defined(recv: &Expr, method: &str) -> bool {
    method == "const_defined?"
        && matches!(&*recv.node, ExprNode::Const { path } if path.iter().map(|p| p.as_str()).filter(|p| !p.is_empty()).eq(["Socket"]))
}

fn name_literal(e: &Expr) -> Option<String> {
    match &*e.node {
        ExprNode::Lit { value: Literal::Sym { value } } => Some(value.as_str().to_string()),
        ExprNode::Lit { value: Literal::Str { value } } => Some(value.clone()),
        _ => None,
    }
}

fn mark_reads(e: &mut Expr, tested: &[String]) {
    if let ExprNode::Const { path } = &*e.node {
        let parts: Vec<&str> = path.iter().map(|p| p.as_str()).filter(|p| !p.is_empty()).collect();
        if let ["Socket", name] = parts.as_slice()
            && tested.iter().any(|t| t == name)
        {
            e.decisions |= crate::expr::PLATFORM_GUARDED_CONST;
        }
    }
    e.node.for_each_child_mut(&mut |c| mark_reads(c, tested));
}
