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
        self.seeded_has_many_read(recv, None).then(|| Ty::Relation { of: target.clone() })
    }

    /// A has_many read the seed takes (`owner.items`, or a bare `items` in the owner's own body): an owner it reads
    /// once, whose reader answers the loaded Array.
    pub(super) fn seeded_has_many_read(&self, read: &Expr, self_ty: Option<&Ty>) -> bool {
        let Some(Ty::Array { elem }) = read.ty.as_ref() else { return false };
        let ExprNode::Send { recv, args, block: None, method: assoc, .. } = &*read.node else { return false };
        if !args.is_empty() {
            return false;
        }
        let owner_ty = match recv {
            Some(owner) if matches!(&*owner.node, ExprNode::Ivar { .. } | ExprNode::Var { .. } | ExprNode::SelfRef)
                || crate::lower::scope_chain::owner_reads_once(owner) => owner.ty.as_ref(),
            Some(_) => return false,
            None => self_ty,
        };
        // A nilable owner (`Article.first`) reads its id as Rails' would, raising on nil.
        let owner_id = match owner_ty {
            Some(Ty::Class { id, .. }) => id,
            Some(Ty::Union { variants }) => match variants.iter().filter(|v| !matches!(v, Ty::Nil)).collect::<Vec<_>>().as_slice() {
                [Ty::Class { id, .. }] => id,
                _ => return false,
            },
            _ => return false,
        };
        self.reads_has_many(owner_id, assoc, elem)
    }

    /// An argument a relation parameter takes (`relation_params`): a has_many read, or a local this body last bound to
    /// one, is lowered to its relation. Other records of a model there are ones Rails' `where` would not answer either.
    pub(super) fn relation_argument(&self, arg: &Expr, ctx: &Ctx) -> Option<Result<Ty, ()>> {
        let is_model = |id: &ClassId| self.classes().get(id).is_some_and(|c| c.table.is_some() || c.active_record_model);
        let Some(Ty::Array { elem }) = arg.ty.as_ref() else {
            // A union holding records may be them at run time, and they answer no `where`.
            let records = matches!(arg.ty.as_ref(), Some(Ty::Union { variants }) if variants.iter().any(|v| matches!(v,
                Ty::Array { elem } if matches!(&**elem, Ty::Class { id, .. } if is_model(id)))));
            return records.then_some(Err(()));
        };
        let Ty::Class { id: target, .. } = &**elem else { return None };
        let self_ty = if ctx.class_side { None } else { ctx.self_ty.as_ref() };
        let seeded = match &*arg.node {
            ExprNode::Var { name, .. } => ctx.has_many_locals.contains_key(name),
            _ => self.seeded_has_many_read(arg, self_ty),
        };
        if seeded {
            return Some(Ok(Ty::Relation { of: target.clone() }));
        }
        is_model(target).then_some(Err(()))
    }

    /// The call whose every return is its relation argument or a chain on it answers that argument's relation.
    pub(super) fn returned_relation_argument(&self, method: &Symbol, args: &[Expr]) -> Option<Ty> {
        let params = self.relation_params?;
        args.iter().enumerate().find_map(|(i, a)| match a.ty.as_ref() {
            Some(rel @ Ty::Relation { .. }) if params.returns(method, i) => Some(rel.clone()),
            _ => None,
        })
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
            || method.as_str() == "all"
            // Not a scope that loads its records (`last(2)`): Rails answers that one's Array.
            || self.classes().get(target).is_some_and(|c| {
                !c.materializing_scopes.contains(method) && matches!(c.class_methods.get(method), Some(Ty::Array { .. } | Ty::Relation { .. }))
            })
    }
}
