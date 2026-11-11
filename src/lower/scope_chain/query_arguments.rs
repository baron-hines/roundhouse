use crate::det::{HashMap, HashSet};

use crate::dialect::{LibraryClass, MethodDef, MethodReceiver, Param};
use crate::expr::{Expr, ExprNode, LValue};
use crate::ident::{ClassId, Symbol};

use super::{Ctx, Registries};

type MethodKey = (ClassId, MethodReceiver, Symbol);

struct Method<'a> {
    params: Vec<Symbol>,
    body: &'a Expr,
}

pub(crate) struct QueryArguments<'a> {
    methods: HashMap<MethodKey, Method<'a>>,
    queries: HashMap<MethodKey, HashSet<usize>>,
}

impl<'a> QueryArguments<'a> {
    pub(crate) fn build(app: &'a crate::App) -> Self {
        let mut result = Self {
            methods: HashMap::default(),
            queries: HashMap::default(),
        };
        for model in &app.models {
            for method in model.methods() {
                result.insert(&model.name, method);
            }
        }
        for class in &app.library_classes {
            for method in &class.methods {
                result.insert(&class.name, method);
            }
        }
        for controller in &app.controllers {
            for action in controller.actions() {
                let Some(params) = fixed_params(&action.formal_params()) else {
                    continue;
                };
                result.methods.insert(
                    (
                        controller.name.clone(),
                        MethodReceiver::Instance,
                        action.name.clone(),
                    ),
                    Method {
                        params,
                        body: &action.body,
                    },
                );
            }
            for method in controller.class_methods() {
                result.insert(&controller.name, method);
            }
        }
        // A parameter is a query argument when the method runs a query
        // on it, or passes it where a method it calls takes one. Each
        // body is walked once: what it queries directly, and which of its
        // parameters it passes in each callee position. The rest follows
        // those edges from what is already known, so a body is not
        // re-walked for every parameter a later round finds.
        let mut found: Vec<(MethodKey, usize)> = Vec::new();
        let mut passes: HashMap<(MethodKey, usize), Vec<(MethodKey, usize)>> = HashMap::default();
        for (key, method) in &result.methods {
            let mut blocked = HashSet::default();
            walk(method.body, &mut |expr| match &*expr.node {
                ExprNode::Assign {
                    target: LValue::Var { name, .. },
                    ..
                } => {
                    blocked.insert(name.clone());
                }
                ExprNode::Lambda {
                    params,
                    rest_param,
                    extra_params,
                    ..
                } => {
                    blocked.extend(params.iter().cloned());
                    blocked.extend(rest_param.iter().cloned());
                    blocked.extend(extra_params.iter().map(|p| p.name.clone()));
                }
                _ => {}
            });
            let param_of = |value: &Expr| match &*value.node {
                ExprNode::Var { name, .. } if !blocked.contains(name) => method.params.iter().position(|p| p == name),
                _ => None,
            };
            walk(method.body, &mut |expr| {
                let ExprNode::Send {
                    recv,
                    method: name,
                    args,
                    block,
                    ..
                } = &*expr.node
                else {
                    return;
                };
                if query_operation(name, block.as_ref())
                    && let Some(index) = recv.as_ref().and_then(|receiver| param_of(receiver))
                {
                    found.push((key.clone(), index));
                }
                if let Some(callee) = method_key(recv.as_ref(), name, &key.0, key.1) {
                    for (at, arg) in args.iter().enumerate() {
                        if let Some(index) = param_of(arg) {
                            passes.entry((callee.clone(), at)).or_default().push((key.clone(), index));
                        }
                    }
                }
            });
        }
        while let Some((key, index)) = found.pop() {
            if !result.queries.entry(key.clone()).or_default().insert(index) {
                continue;
            }
            if let Some(callers) = passes.get(&(key, index)) {
                found.extend(callers.iter().cloned());
            }
        }
        result
    }

    fn insert(&mut self, owner: &ClassId, method: &'a MethodDef) {
        if method.unsupported_formals.is_some() {
            return;
        }
        let Some(params) = fixed_params(&method.params) else {
            return;
        };
        self.methods.insert(
            (owner.clone(), method.receiver, method.name.clone()),
            Method {
                params,
                body: &method.body,
            },
        );
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.queries.is_empty()
    }

    pub(crate) fn rewrite(&self, classes: &mut [LibraryClass], regs: &Registries<'_>) {
        for class in classes {
            for method in &mut class.methods {
                let ctx = Ctx {
                    scopes: regs.scopes,
                    models: regs.models,
                    assocs: regs.assocs,
                    assoc_class_methods: regs.assoc_class_methods,
                    user_returns: regs.user_returns,
                    unique_keys: regs.unique_keys,
                    owner_objects: regs.owner_objects,
                    scope_body: None,
                    self_const_is_implicit: false,
                    class_self: (method.receiver == MethodReceiver::Class)
                        .then(|| class.name.clone()),
                    instance_self: (method.receiver == MethodReceiver::Instance)
                        .then(|| class.name.clone()),
                };
                self.rewrite_body(&mut method.body, &class.name, method.receiver, &ctx);
            }
        }
    }

    fn rewrite_body(&self, expr: &mut Expr, owner: &ClassId, side: MethodReceiver, ctx: &Ctx) {
        expr.node
            .for_each_child_mut(&mut |child| self.rewrite_body(child, owner, side, ctx));
        let ExprNode::Send {
            recv, method, args, ..
        } = &mut *expr.node
        else {
            return;
        };
        let Some(key) = method_key(recv.as_ref(), method, owner, side) else {
            return;
        };
        let Some(indices) = self.queries.get(&key) else {
            return;
        };
        for index in indices {
            let Some(arg) = args.get_mut(*index) else {
                continue;
            };
            if let Some(seed) = seed_argument(arg, ctx) {
                *arg = seed;
            }
        }
    }
}

fn fixed_params(params: &[Param]) -> Option<Vec<Symbol>> {
    (!params
        .iter()
        .any(|p| p.keyword || p.rest || p.forwarding || p.from_keyword || p.from_kwrest))
    .then(|| params.iter().map(|p| p.name.clone()).collect())
}

fn method_key(
    recv: Option<&Expr>,
    name: &Symbol,
    owner: &ClassId,
    side: MethodReceiver,
) -> Option<MethodKey> {
    match recv.map(|e| &*e.node) {
        None | Some(ExprNode::SelfRef) => Some((owner.clone(), side, name.clone())),
        Some(ExprNode::Const { path }) => {
            let written = path
                .iter()
                .map(Symbol::as_str)
                .collect::<Vec<_>>()
                .join("::");
            if let Some(crate::ty::Ty::Class { id, .. }) = recv.and_then(|r| r.ty.as_ref()) {
                return (id.0.as_str() == written
                    || id.0.as_str().ends_with(&format!("::{written}")))
                .then(|| (id.clone(), MethodReceiver::Class, name.clone()));
            }
            (!owner.0.as_str().contains("::") || path.len() > 1).then(|| {
                (
                    ClassId(Symbol::from(written)),
                    MethodReceiver::Class,
                    name.clone(),
                )
            })
        }
        _ => None,
    }
}

fn query_operation(name: &Symbol, block: Option<&Expr>) -> bool {
    (block.is_none() && super::is_relation_chain_method(name.as_str()))
        || matches!(
            name.as_str(),
            "find_by"
                | "find_by!"
                | "find_sole_by"
                | "find_each"
                | "find_in_batches"
                | "in_batches"
                | "to_sql"
                | "update_all"
                | "delete_all"
                | "destroy_all"
                | "touch_all"
        )
}

fn seed_argument(value: &Expr, ctx: &Ctx) -> Option<Expr> {
    let value = super::self_qualified_assoc_read(value.clone(), ctx);
    let ExprNode::Send {
        recv: Some(owner),
        method: name,
        args,
        block: None,
        ..
    } = &*value.node
    else {
        return None;
    };
    if !args.is_empty() || super::seedable_assoc_read(ctx, &value, value.span).is_none() {
        return None;
    }
    let (owner_model, target, fk, owner_id) = if matches!(&*owner.node, ExprNode::SelfRef) {
        let model = ctx.instance_self.as_ref()?;
        let (target, fk) = ctx.assocs.has_many_fk(model, name)?.clone();
        let key = ctx.assocs.has_many_owner_key(Some(model), name);
        (
            Some(model.clone()),
            target,
            fk,
            super::syn(value.span, ExprNode::Ivar { name: key }),
        )
    } else {
        let owner_model =
            super::assoc_read_target(owner, name, ctx.assocs).and_then(|(model, _, _)| model);
        let (target, fk, owner_id) = super::assoc_owner_seed(ctx, name, owner, value.span)?;
        (owner_model, target, fk, owner_id)
    };
    let type_entry = ctx
        .assocs
        .has_many_poly(owner_model.as_ref(), name)
        .map(|(column, model)| {
            (
                super::syn(
                    value.span,
                    ExprNode::Lit {
                        value: crate::expr::Literal::Sym {
                            value: column.clone(),
                        },
                    },
                ),
                super::syn(
                    value.span,
                    ExprNode::Lit {
                        value: crate::expr::Literal::Str {
                            value: model.clone(),
                        },
                    },
                ),
            )
        });
    Some(super::build_association_seed(
        value.span, &target, fk, owner_id, type_entry, owner, name, false,
    ))
}

fn walk(expr: &Expr, visit: &mut impl FnMut(&Expr)) {
    visit(expr);
    expr.node.for_each_child(&mut |child| walk(child, visit));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(report: &str) -> crate::App {
        let tree = [
            ("db/schema.rb", "ActiveRecord::Schema.define do\n  create_table :members do |t|\n    t.integer :parent_id\n  end\nend\n"),
            ("app/models/member.rb", "class Member < ActiveRecord::Base\nend\n"),
            ("app/models/report.rb", report),
        ]
        .into_iter()
        .map(|(path, source)| (std::path::PathBuf::from(path), source.as_bytes().to_vec()))
        .collect();
        crate::ingest::ingest_app_from_tree(tree).expect("ingest")
    }

    fn queried(arguments: &QueryArguments<'_>, method: &str) -> Vec<usize> {
        let key = (ClassId(Symbol::from("Report")), MethodReceiver::Instance, Symbol::from(method));
        let mut indices: Vec<usize> = arguments.queries.get(&key).into_iter().flatten().copied().collect();
        indices.sort();
        indices
    }

    #[test]
    fn a_parameter_passed_on_to_a_query_through_several_calls_is_a_query_argument() {
        let app = app(
            "class Report\n  def top(a, b)\n    middle(b, a)\n  end\n\n  def middle(x, y)\n    bottom(1, x)\n  end\n\n  def bottom(n, rows)\n    rows.order(:id)\n  end\n\n  def shadowed(rows)\n    rows = []\n    bottom(1, rows)\n  end\nend\n",
        );
        let arguments = QueryArguments::build(&app);
        assert_eq!(queried(&arguments, "bottom"), [1]);
        assert_eq!(queried(&arguments, "middle"), [0]);
        assert_eq!(queried(&arguments, "top"), [1]);
        assert!(queried(&arguments, "shadowed").is_empty());
    }
}
