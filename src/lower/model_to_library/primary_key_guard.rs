//! Fold the save guard after a concrete model's inherited methods are known.
//! Do not use the runtime Base's return type: a subclass may override its key.

use crate::dialect::{MethodDef, MethodReceiver};
use crate::expr::{Expr, ExprNode, LValue, Literal};

pub(crate) fn fold(methods: &mut [MethodDef]) {
    let Some(key) = methods.iter().rev().find(|m| {
        m.receiver == MethodReceiver::Class && m.name.as_str() == "primary_key" && m.params.is_empty()
    }) else { return };
    let nil = match &*key.body.node {
        ExprNode::Lit { value: Literal::Nil } => true,
        ExprNode::Lit { value: Literal::Str { .. } } => false,
        _ => return,
    };
    for method in methods.iter_mut().filter(|m| {
        m.receiver == MethodReceiver::Instance && m.name.as_str() == "save_after_validation"
    }) {
        fold_guard(&mut method.body, nil);
        // Folding a keyless save leaves the insert's result unused. Keep
        // the insert, without a binding that typed targets warn about.
        if nil && !reads_inserted_key(&method.body) {
            discard_insert_binding(&mut method.body);
        }
    }
}

fn is_key_nil(expr: &Expr) -> bool {
    let ExprNode::Send { recv: Some(key), method, args, block: None, .. } = &*expr.node else { return false };
    if method.as_str() != "nil?" || !args.is_empty() { return false }
    let ExprNode::Send { recv: Some(class), method, args, block: None, .. } = &*key.node else { return false };
    if method.as_str() != "primary_key" || !args.is_empty() { return false }
    matches!(&*class.node, ExprNode::Send { recv: Some(receiver), method, args, block: None, .. }
        if method.as_str() == "class" && args.is_empty() && matches!(&*receiver.node, ExprNode::SelfRef))
}

fn fold_guard(expr: &mut Expr, nil: bool) {
    expr.node.for_each_child_mut(&mut |child| fold_guard(child, nil));
    if let ExprNode::If { cond, then_branch, else_branch } = &*expr.node
        && is_key_nil(cond)
    {
        *expr = if nil { then_branch.clone() } else { else_branch.clone() };
    }
    if let ExprNode::Seq { exprs } = &mut *expr.node
        && let Some(tail) = exprs.pop()
    {
        exprs.retain(|e| !matches!(&*e.node, ExprNode::Lit { value: Literal::Nil }));
        exprs.push(tail);
    }
}

fn reads_inserted_key(expr: &Expr) -> bool {
    let mut read = matches!(&*expr.node, ExprNode::Var { name, .. } if name.as_str() == "inserted_key");
    expr.node.for_each_child(&mut |child| read |= reads_inserted_key(child));
    read
}

fn discard_insert_binding(expr: &mut Expr) {
    expr.node.for_each_child_mut(&mut discard_insert_binding);
    if let ExprNode::Assign { target: LValue::Var { name, .. }, value } = &*expr.node
        && name.as_str() == "inserted_key"
        && matches!(&*value.node, ExprNode::Send { method, args, block: None, .. }
            if method.as_str() == "_adapter_insert" && args.is_empty())
    {
        *expr = value.clone();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn methods(key: &str) -> Vec<MethodDef> {
        crate::runtime_src::parse_methods(&format!(
            "class Widget\n  def self.primary_key\n    {key}\n  end\n  def save_after_validation\n    inserted_key = _adapter_insert\n    self.id = inserted_key unless self.class.primary_key.nil?\n  end\nend\n"
        )).unwrap()
    }

    fn count(expr: &Expr, predicate: &dyn Fn(&Expr) -> bool) -> usize {
        let mut n = usize::from(predicate(expr));
        expr.node.for_each_child(&mut |child| n += count(child, predicate));
        n
    }

    #[test]
    fn literal_keys_keep_one_insert_and_only_keyed_models_assign_the_id() {
        for (key, setters) in [("\"id\"", 1), ("\"code\"", 1), ("nil", 0)] {
            let mut methods = methods(key);
            fold(&mut methods);
            let body = &methods.iter().find(|m| m.name.as_str() == "save_after_validation").unwrap().body;
            assert_eq!(count(body, &is_key_nil), 0, "{key}: {body:?}");
            assert_eq!(count(body, &|e| matches!(&*e.node, ExprNode::Send { method, .. } if method.as_str() == "_adapter_insert")), 1);
            assert_eq!(count(body, &|e| {
                matches!(&*e.node, ExprNode::Assign { target: LValue::Attr { name, .. }, .. } if name.as_str() == "id")
                    || matches!(&*e.node, ExprNode::Send { method, .. } if method.as_str() == "id=")
            }), setters, "{key}: {body:?}");
            if setters == 0 {
                assert_eq!(count(body, &|e| matches!(&*e.node, ExprNode::Assign { target: LValue::Var { name, .. }, .. } if name.as_str() == "inserted_key")), 0);
            }
        }
    }

    #[test]
    fn a_nonliteral_key_keeps_the_runtime_guard() {
        let mut methods = methods("lookup_key");
        let before = methods.clone();
        fold(&mut methods);
        assert_eq!(methods, before);
    }
}
