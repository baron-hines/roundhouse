//! A scope or query method on an association read is typed as the target's relation: the lowering seeds it as
//! `Relation.new(Target).where(fk: owner.id)` (`scope_chain`'s association seed), as Rails' CollectionProxy is one.

use super::*;

impl<'a> BodyTyper<'a> {
    /// `owner.accounts.visible` → `Relation[Account]`, where dispatch answered the association's Array. A bare
    /// association read stays the loaded Array, and so does an owner the seed does not take.
    pub(super) fn association_scope_relation(&self, recv: &Expr, method: &Symbol, dispatched: &Ty) -> Option<Ty> {
        let Ty::Array { elem } = dispatched else { return None };
        let Ty::Class { id: target, .. } = &**elem else { return None };
        let Some(Ty::Array { elem: recv_elem }) = recv.ty.as_ref() else { return None };
        if **recv_elem != **elem || !self.is_relation_step(target, method) {
            return None;
        }
        let ExprNode::Send { recv: Some(owner), args, block: None, method: assoc, .. } = &*recv.node else { return None };
        // A nilable owner (`Article.first`) reads its id as Rails' would, raising on nil.
        let owner_id = match owner.ty.as_ref() {
            Some(Ty::Class { id, .. }) => id,
            Some(Ty::Union { variants }) => match variants.iter().filter(|v| !matches!(v, Ty::Nil)).collect::<Vec<_>>().as_slice() {
                [Ty::Class { id, .. }] => id,
                _ => return None,
            },
            _ => return None,
        };
        let seeded = args.is_empty()
            && (matches!(&*owner.node, ExprNode::Ivar { .. } | ExprNode::Var { .. } | ExprNode::SelfRef)
                || crate::lower::scope_chain::owner_reads_once(owner))
            && self.reads_has_many(owner_id, assoc, elem);
        seeded.then(|| Ty::Relation { of: target.clone() })
    }

    /// The owner's has_many reader: one whose `<name>_loaded?` predicate is synthesized beside it, answering the
    /// target's Array, as `lower::assoc_loaded` recognizes it.
    fn reads_has_many(&self, owner: &ClassId, assoc: &Symbol, elem: &Ty) -> bool {
        let flat = Symbol::from(format!("{}_loaded?", assoc.as_str()));
        let mut current = Some(owner);
        for _ in 0..32 {
            let Some(cid) = current else { return false };
            let Some(cls) = self.classes().get(cid) else { return false };
            if cls.instance_methods.contains_key(&flat) {
                return matches!(cls.instance_methods.get(assoc), Some(Ty::Array { elem: e }) if **e == *elem);
            }
            current = cls.parent.as_ref();
        }
        false
    }

    /// A chain step that keeps a relation: a query method, or a class method of the element model answering one.
    fn is_relation_step(&self, target: &ClassId, method: &Symbol) -> bool {
        crate::lower::scope_chain::is_relation_chain_method(method.as_str())
            // Not a scope that loads its records (`last(2)`): Rails answers that one's Array.
            || self.classes().get(target).is_some_and(|c| {
                !c.materializing_scopes.contains(method) && matches!(c.class_methods.get(method), Some(Ty::Array { .. } | Ty::Relation { .. }))
            })
    }
}
