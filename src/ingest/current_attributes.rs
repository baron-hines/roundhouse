//! `class Current < ActiveSupport::CurrentAttributes` → a plain,
//! statically-resolvable class.
//!
//! Rails' CurrentAttributes is three pieces of metaprogramming stacked:
//! `attribute :a, :b` defines instance accessors into a generated module,
//! the class-level `Current.user` is `method_missing` forwarding to a
//! per-thread singleton, and an app's own `def user=` reaches the
//! generated writer through `super`. None of that survives into an
//! emitted tree — the runtime has no `ActiveSupport::CurrentAttributes`,
//! and a target that resolves calls statically cannot follow
//! `method_missing` at all ([[feedback_runtime_must_be_statically_resolvable]]).
//!
//! campfire routes essentially everything through it (`Current.user`,
//! `Current.account`, `Current.session`), so this is load-bearing rather
//! than a nicety: without it the class replays verbatim and every
//! `Current.x` is unresolvable.
//!
//! ## What it becomes
//!
//! * one `@__instance` singleton plus `self.instance` / `self.reset`;
//! * a reader and writer per declared attribute, storing in an ivar;
//! * `delegate … to: :x, prefix: true, allow_nil: true` expanded to real
//!   methods (`request_host` → `@request.host` unless `@request` is nil; a
//!   target that is a method, not an attribute, is called, as Rails does);
//! * a `self.<name>` forwarder for every instance method, which is what
//!   makes the class-level call sites app code actually writes resolve.
//!
//! ## `super` in an app-written writer
//!
//! campfire writes `def session=(value); super(value); …; end`. Rather
//! than synthesize a module for the app's `super` to find — a mixin is
//! Ruby-family-only, and the strict targets have no equivalent — the
//! `super` call is REWRITTEN to the storage write it means (`@session =
//! value`). Same reasoning as the concern splices: resolve it in the IR
//! once, for all thirteen targets.

use crate::dialect::{LibraryClass, MethodDef, MethodReceiver};
use crate::expr::{Expr, ExprNode, Literal};
use crate::ident::{ClassId, Symbol};

const BASE: &str = "ActiveSupport::CurrentAttributes";

/// Rewrite every `Current`-shaped class in the app.
pub fn lower_current_attributes(app: &mut crate::App) {
    let mut generated: Vec<(usize, Vec<MethodDef>)> = Vec::new();
    let mut lowered: Vec<ClassId> = Vec::new();
    let mut signatures: Vec<(ClassId, Symbol, crate::ty::Ty)> = Vec::new();
    let mut refusals = Vec::new();
    for (i, lc) in app.library_classes.iter_mut().enumerate() {
        if lc.parent.as_ref().map(|p| p.0.as_str()) != Some(BASE) {
            continue;
        }
        let (attrs, declared, defaults) = take_attribute_decls(lc, &mut refusals);
        let delegates = take_delegate_decls(lc);
        rewrite_super_accessors(lc, &attrs);
        let src = synthesized_source(lc, &attrs, &delegates);
        // Isolated in its own `prism::scope` — never the outer one
        // that spans the whole app's ingest — so a bug in
        // `synthesized_source` can't render its parse errors against
        // an unrelated real file (see `ingest::sources`'s module doc).
        let (parsed, diags) = crate::ingest::prism::scope(|| {
            crate::ingest::ingest_library_classes(src.as_bytes(), "<current>")
        });
        let methods = match parsed {
            Ok(classes) if diags.is_empty() => {
                let mut methods: Vec<MethodDef> = classes.into_iter().flat_map(|c| c.methods).collect();
                place_defaults(&mut methods, &defaults);
                for (name, signature) in declare_attribute_types(&mut methods, &declared) {
                    signatures.push((lc.name.clone(), name, signature));
                }
                methods
            }
            Ok(_) => {
                super::survey::record_synthesis_failure(
                    "<current>",
                    &format!("CurrentAttributes forwarder for `{}`", lc.name.0.as_str()),
                    &diags,
                );
                Vec::new()
            }
            Err(err) => {
                super::survey::record(&err);
                Vec::new()
            }
        };
        // The base is gone: nothing is inherited any more. Record the
        // name first — clearing the parent is exactly what makes this
        // class unrecognizable to everything downstream.
        lc.parent = None;
        lowered.push(lc.name.clone());
        generated.push((i, methods));
    }
    for (i, methods) in generated {
        app.library_classes[i].methods.extend(methods);
    }
    // Declared like an inline signature, so the analysis reads the
    // written type in the class's lexical scope (`Context::…`
    // inside a namespaced `Current`).
    for (class, name, signature) in signatures {
        app.rbs_signatures.entry(class).or_default().entry(name).or_insert(signature);
    }
    // Extended, not replaced: a later pass (`acts_as_tenant`) lowers a
    // class it synthesizes the same way.
    app.current_attribute_classes.extend(lowered);
    app.current_attribute_refusals.extend(refusals);
}

/// `attribute :session, :user, :request` — consumed, not replayed.
/// An attribute may carry its type in a trailing RBS comment
/// (`attribute :tenant #: Account::Context::Impl`),
/// which ingest reads as a cast around the call: that type is returned
/// with each name the call lists. Without unwrapping it the declaration was not taken,
/// the class got no accessor, and `Current.tenant` had no method.
fn take_attribute_decls(lc: &mut LibraryClass, refusals: &mut Vec<crate::diagnostic::Diagnostic>) -> (Vec<Symbol>, Vec<(Symbol, crate::ty::Ty)>, Vec<(Symbol, Expr)>) {
    let mut out = Vec::new();
    let mut declared = Vec::new();
    let mut defaults = Vec::new();
    lc.unknown_calls.retain(|call| {
        let (call, declared_ty) = match &*call.node {
            ExprNode::Cast { value, target_ty } => (value, Some(target_ty)),
            _ => (call, None),
        };
        let ExprNode::Send { recv: None, method, args, .. } = &*call.node else { return true };
        if method.as_str() != "attribute" {
            return true;
        }
        let names: Vec<Symbol> = args
            .iter()
            .filter_map(|a| match &*a.node {
                ExprNode::Lit { value: Literal::Sym { value } } => Some(value.clone()),
                _ => None,
            })
            .collect();
        let mut default = None;
        for arg in args {
            let ExprNode::Hash { entries, .. } = &*arg.node else { continue };
            for (key, value) in entries {
                let reproduced = matches!(&*key.node, ExprNode::Lit { value: Literal::Sym { value } } if value.as_str() == "default")
                    .then(|| default_value(value))
                    .flatten();
                match reproduced {
                    Some(value) => default = Some(value),
                    // Consumed all the same: replayed on the now-plain class, the call would raise when it loads.
                    None => refusals.push(crate::diagnostic::Diagnostic::unsupported(
                        key.span,
                        None,
                        "CurrentAttributes attribute option",
                        "only `default:` with a literal or a no-argument callable is reproduced; the attribute reads nil instead",
                    )),
                }
            }
        }
        if let Some(ty) = declared_ty {
            declared.extend(names.iter().map(|name| (name.clone(), ty.clone())));
        }
        defaults.retain(|(name, _): &(Symbol, Expr)| !names.contains(name));
        if let Some(value) = default {
            defaults.extend(names.iter().map(|name| (name.clone(), value.clone())));
        }
        out.extend(names);
        false
    });
    (out, declared, defaults)
}

/// What `resolve_defaults` assigns: a callable's result, or a fresh copy of a literal.
/// A literal written in place is already fresh on each evaluation, which is Rails' `dup`.
/// Anything else (a constant, a method call) has no `dup`-equivalent shape here.
fn default_value(value: &Expr) -> Option<Expr> {
    fn scalar(e: &Expr) -> bool {
        matches!(&*e.node, ExprNode::Lit { .. })
    }
    // Nested containers are refused: Rails' shallow `dup` shares them, a literal written in place would not.
    let literal = |e: &Expr| match &*e.node {
        ExprNode::Lit { .. } => true,
        ExprNode::Array { elements, .. } => elements.iter().all(scalar),
        ExprNode::Hash { entries, .. } => entries.iter().all(|(k, v)| scalar(k) && scalar(v)),
        _ => false,
    };
    let nullary = |e: &Expr| match &*e.node {
        ExprNode::Lambda { params, rest_param: None, extra_params, block_param: None, body, .. }
            if params.is_empty() && extra_params.is_empty() => callable_body(body),
        _ => None,
    };
    if literal(value) {
        return Some(value.clone());
    }
    if let Some(body) = nullary(value) {
        return Some(body);
    }
    match &*value.node {
        ExprNode::Send { recv: None, method, args, block: Some(block), .. }
            if args.is_empty() && matches!(method.as_str(), "proc" | "lambda") => nullary(block),
        _ => None,
    }
}

/// Inlined into `__reset_attributes`, `self`/ivars/receiverless calls would read the instance rather than the closure's class, a captured local would be unbound, and a non-final `return` would leave reset.
fn callable_body(body: &Expr) -> Option<Expr> {
    fn context_free(e: &Expr) -> bool {
        let here = !matches!(
            &*e.node,
            ExprNode::SelfRef
                | ExprNode::Ivar { .. }
                | ExprNode::Return { .. }
                | ExprNode::Yield { .. }
                | ExprNode::Super { .. }
                | ExprNode::Send { recv: None, .. }
        );
        let mut children = true;
        e.node.for_each_child(&mut |c| children &= context_free(c));
        here && children
    }
    fn assigned(e: &Expr, out: &mut Vec<Symbol>) {
        if let ExprNode::Assign { target: crate::expr::LValue::Var { name, .. }, .. } = &*e.node {
            out.push(name.clone());
        }
        e.node.for_each_child(&mut |c| assigned(c, out));
    }
    fn captures(e: &Expr, own: &[Symbol]) -> bool {
        let here = matches!(&*e.node, ExprNode::Var { name, .. } if !own.contains(name));
        let mut children = false;
        e.node.for_each_child(&mut |c| children |= captures(c, own));
        here || children
    }
    let mut own = Vec::new();
    assigned(body, &mut own);
    if captures(body, &own) {
        return None;
    }
    let mut body = body.clone();
    let last = match &mut *body.node {
        ExprNode::Seq { exprs } => exprs.last_mut(),
        _ => Some(&mut body),
    };
    if let Some(last) = last {
        if let ExprNode::Return { value } = &*last.node {
            *last = value.clone();
        }
    }
    context_free(&body).then_some(body)
}

/// The synthesized `__reset_attributes` assigns nil; a declared default replaces that value.
fn place_defaults(methods: &mut [MethodDef], defaults: &[(Symbol, Expr)]) {
    let Some(reset) = methods.iter_mut().find(|m| m.receiver == MethodReceiver::Instance && m.name.as_str() == RESET_ATTRIBUTES) else { return };
    fn visit(e: &mut Expr, defaults: &[(Symbol, Expr)]) {
        if let ExprNode::Assign { target: crate::expr::LValue::Ivar { name }, value } = &mut *e.node {
            if let Some((_, default)) = defaults.iter().find(|(a, _)| a == name) {
                *value = default.clone();
            }
            return;
        }
        e.node.for_each_child_mut(&mut |c| visit(c, defaults));
    }
    visit(&mut reset.body, defaults);
}

const RESET_ATTRIBUTES: &str = "__reset_attributes";

/// The synthesized reader answers, and the writer takes, an
/// attribute's declared type, nilable: every attribute reads nil until
/// the request sets it (and after `reset`), and callers ask exactly that
/// (`create_tenant unless Current.tenant`). Read as written,
/// the presence test would fold to true (see current_attributes_typing).
/// Returns each accessor's signature.
fn declare_attribute_types(methods: &mut [MethodDef], declared: &[(Symbol, crate::ty::Ty)]) -> Vec<(Symbol, crate::ty::Ty)> {
    use crate::ty::{FnTy, Param, ParamKind, Ty};
    let mut out = Vec::new();
    for (name, written) in declared {
        let ty = &match written {
            Ty::Nil => Ty::Nil,
            Ty::Union { variants } if variants.contains(&Ty::Nil) => written.clone(),
            _ => Ty::Union { variants: vec![written.clone(), Ty::Nil].into() },
        };
        let writer = format!("{}=", name.as_str());
        for method in methods.iter_mut().filter(|m| m.receiver == MethodReceiver::Instance) {
            let params = if method.name == *name {
                vec![]
            } else if method.name.as_str() == writer {
                vec![Param { name: Symbol::from("value"), ty: ty.clone().into(), kind: ParamKind::Required }].into()
            } else {
                continue;
            };
            let signature = Ty::Fn(Box::new(FnTy { params: params.into(), block: None, ret: std::sync::Arc::new(ty.clone()), effects: method.effects.clone() }));
            method.signature = Some(signature.clone());
            out.push((method.name.clone(), signature));
        }
    }
    out
}

/// A delegate target Ruby would parse as a keyword (`to: :class`), which Rails' delegate qualifies with `self.`.
const RUBY_KEYWORDS: [&str; 41] = [
    "__ENCODING__", "__LINE__", "__FILE__", "BEGIN", "END", "alias", "and", "begin", "break", "case", "class", "def",
    "defined?", "do", "else", "elsif", "end", "ensure", "false", "for", "if", "in", "module", "next", "nil", "not", "or",
    "redo", "rescue", "retry", "return", "self", "super", "then", "true", "undef", "unless", "until", "when", "while",
    "yield",
];

/// `delegate :host, :protocol, to: :request, prefix: true, allow_nil: true`.
/// Returns `(method, target, generated_name, allow_nil)`.
struct Delegation {
    method: Symbol,
    target: Symbol,
    name: String,
    allow_nil: bool,
}

fn take_delegate_decls(lc: &mut LibraryClass) -> Vec<Delegation> {
    let mut out = Vec::new();
    lc.unknown_calls.retain(|call| {
        let ExprNode::Send { recv: None, method, args, .. } = &*call.node else { return true };
        if method.as_str() != "delegate" {
            return true;
        }
        let mut names: Vec<Symbol> = Vec::new();
        let (mut to, mut prefix, mut allow_nil) = (None, false, false);
        for a in args {
            match &*a.node {
                ExprNode::Lit { value: Literal::Sym { value } } => names.push(value.clone()),
                ExprNode::Hash { entries, .. } => {
                    for (k, v) in entries {
                        let ExprNode::Lit { value: Literal::Sym { value: key } } = &*k.node else {
                            continue;
                        };
                        match key.as_str() {
                            "to" => {
                                if let ExprNode::Lit { value: Literal::Sym { value } } = &*v.node {
                                    to = Some(value.clone());
                                }
                            }
                            "prefix" => prefix = matches!(&*v.node,
                                ExprNode::Lit { value: Literal::Bool { value: true } }),
                            "allow_nil" => allow_nil = matches!(&*v.node,
                                ExprNode::Lit { value: Literal::Bool { value: true } }),
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }
        // Only the shape we can reproduce exactly. Anything else stays
        // in `unknown_calls`, visible, rather than half-expanded.
        let Some(target) = to else { return true };
        for m in names {
            let name = if prefix {
                format!("{}_{}", target.as_str(), m.as_str())
            } else {
                m.as_str().to_string()
            };
            out.push(Delegation { method: m, target: target.clone(), name, allow_nil });
        }
        false
    });
    out
}

/// `super` inside an app-written `<attr>`, `<attr>=`, `reset` or
/// `initialize` is the generated method it overrides: the storage read or
/// write, or the defaults reset. Rewrite it in place so no superclass is needed.
fn rewrite_super_accessors(lc: &mut LibraryClass, attrs: &[Symbol]) {
    for m in &mut lc.methods {
        if m.receiver != MethodReceiver::Instance {
            continue;
        }
        let name = m.name.as_str();
        let target = if matches!(name, "reset" | "initialize") {
            SuperTarget::Reset
        } else if let Some(attr) = name.strip_suffix('=').filter(|a| attrs.iter().any(|x| x.as_str() == *a)) {
            // A bare `super` forwards the writer's own parameter.
            let param = m.params.first().map(|p| p.name.clone());
            SuperTarget::Write(Symbol::from(attr), param)
        } else if attrs.iter().any(|a| a.as_str() == name) {
            SuperTarget::Read(Symbol::from(name))
        } else {
            continue;
        };
        rewrite_super(&mut m.body, &target);
    }
}

enum SuperTarget {
    Read(Symbol),
    Write(Symbol, Option<Symbol>),
    Reset,
}

fn rewrite_super(expr: &mut Expr, target: &SuperTarget) {
    expr.node.for_each_child_mut(&mut |c| rewrite_super(c, target));
    let ExprNode::Super { args } = &*expr.node else { return };
    let span = expr.span;
    let node = match target {
        SuperTarget::Read(ivar) if args.as_ref().is_none_or(|a| a.is_empty()) => ExprNode::Ivar { name: ivar.clone() },
        SuperTarget::Write(ivar, param) => {
            let value = match args {
                Some(a) if a.len() == 1 => a[0].clone(),
                None => match param {
                    Some(p) => Expr::new(span, ExprNode::Var { id: crate::ident::VarId(0), name: p.clone() }),
                    None => return,
                },
                _ => return,
            };
            ExprNode::Assign { target: crate::expr::LValue::Ivar { name: ivar.clone() }, value }
        }
        SuperTarget::Reset if args.as_ref().is_none_or(|a| a.is_empty()) => ExprNode::Send {
            recv: None,
            method: Symbol::from(RESET_ATTRIBUTES),
            args: Vec::new(),
            block: None,
            parenthesized: false,
        },
        _ => return,
    };
    *expr = Expr::new(span, node);
}

/// Build the synthesized half as Ruby and let ingest parse it, rather
/// than hand-assembling three dozen `Expr` trees.
fn synthesized_source(
    lc: &LibraryClass,
    attrs: &[Symbol],
    delegates: &[Delegation],
) -> String {
    let class = lc.name.0.as_str();
    let defines = |name: &str| {
        lc.methods
            .iter()
            .any(|m| m.receiver == MethodReceiver::Instance && m.name.as_str() == name)
    };

    let mut body = String::new();
    // ONE INSTANCE PER THREAD, which is what `CurrentAttributes` means:
    // Rails keeps the instance in `IsolatedExecutionState`, so two
    // requests served at once each see their own `Current.user`. A
    // class-level `@__instance` was one instance for the whole process
    // -- correct under the single-fiber server the binary used to run,
    // and a cross-request identity leak the moment two green threads (or
    // two Puma threads on the CRuby lane) dispatch at once. The slot is
    // `Thread.current[...]`, keyed per class so two CurrentAttributes
    // classes in one app do not share it; spinel types thread-local
    // storage by key from its assignments, the way it types ivars, so
    // the reader below narrows to the class and every forwarder stays
    // typed.
    //
    // `reset` REPLACES the instance rather than nilling it. Rails'
    // `CurrentAttributes.reset` sets every attribute back to its
    // default, which a fresh instance is -- and a reader that ever
    // answered a nilable made `Current.instance.user` fail to dispatch
    // and EVERY class-level forwarder register `Untyped`. The reader
    // narrows its slot read behind `is_a?`, not `nil?`, for the same
    // reason: `Thread#[]` answers untyped, and a `nil?` test leaves it so.
    let key = format!("__current_attrs_{}", class.replace("::", "_"));
    body.push_str(&format!(
        "  def self.instance\n    c = Thread.current[:{key}]\n    return c if c.is_a?({class})\n    c = {class}.new\n    Thread.current[:{key}] = c\n    c\n  end\n\n"
    ));
    // Rails' `reset` keeps the instance and reassigns each attribute its default, without the app's writers.
    let mut assigns: String = attrs.iter().map(|a| format!("    @{} = nil\n", a.as_str())).collect();
    assigns.push_str("    nil\n");
    body.push_str(&format!("  def {RESET_ATTRIBUTES}\n{assigns}  end\n\n"));
    if !defines("initialize") {
        body.push_str(&format!("  def initialize\n    {RESET_ATTRIBUTES}\n  end\n\n"));
    }
    if !defines("reset") {
        body.push_str(&format!("  def reset\n    {RESET_ATTRIBUTES}\n  end\n\n"));
    }

    for a in attrs {
        let a = a.as_str();
        if !defines(a) {
            body.push_str(&format!("  def {a}\n    @{a}\n  end\n\n"));
        }
        if !defines(&format!("{a}=")) {
            body.push_str(&format!("  def {a}=(value)\n    @{a} = value\n  end\n\n"));
        }
    }

    for d in delegates {
        if defines(&d.name) {
            continue;
        }
        let (t, m) = (d.target.as_str(), d.method.as_str());
        // `to: :user` calls the method: the ivar only for a declared attribute whose reader the app left as storage.
        let target = if t.starts_with('@') {
            t.to_string()
        } else if attrs.iter().any(|a| a.as_str() == t) && !defines(t) {
            format!("@{t}")
        } else if RUBY_KEYWORDS.contains(&t) {
            format!("self.{t}")
        } else if t.trim_start_matches("::").starts_with(|c: char| c.is_ascii_uppercase()) {
            t.to_string()
        } else {
            // Not bare: `__target = __target` would read the local being assigned, not the method.
            format!("{t}()")
        };
        if d.allow_nil {
            body.push_str(&format!(
                "  def {}\n    __target = {target}\n    __target.nil? ? nil : __target.{m}\n  end\n\n",
                d.name
            ));
        } else {
            body.push_str(&format!("  def {}\n    {target}.{m}\n  end\n\n", d.name));
        }
    }

    // Class-level forwarders — the surface app code actually calls
    // (`Current.user`, `Current.account`). Rails gets these from
    // `method_missing`; a statically-resolved target needs them real.
    let mut forwarded: Vec<String> = vec!["reset".to_string()];
    for a in attrs {
        forwarded.push(a.as_str().to_string());
        forwarded.push(format!("{}=", a.as_str()));
    }
    for d in delegates {
        forwarded.push(d.name.clone());
    }
    for m in &lc.methods {
        if m.receiver == MethodReceiver::Instance && m.name.as_str() != "initialize" {
            forwarded.push(m.name.as_str().to_string());
        }
    }
    forwarded.sort();
    forwarded.dedup();
    for name in forwarded {
        if let Some(attr) = name.strip_suffix('=') {
            body.push_str(&format!(
                "  def self.{attr}=(value)\n    {class}.instance.{attr} = value\n  end\n\n"
            ));
        } else {
            body.push_str(&format!(
                "  def self.{name}\n    {class}.instance.{name}\n  end\n\n"
            ));
        }
    }

    format!("class {class}\n{body}end\n")
}

