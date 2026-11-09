//! What a gem's code does that the generic passes cannot model, written
//! here once per gem rather than in the passes. Each hook is keyed on the
//! gem's own names and applies only where they stand, so an app needs no
//! setting for it.

use crate::App;
use crate::det::{HashMap, HashSet};
use crate::expr::{Expr, ExprNode, LValue, Literal};

use super::configured_superclass::Val;

/// What the evaluator does with a call on the routing mapper in place of recording it.
pub(super) enum RouterCall {
    /// Runs the call's block as though the call were not there.
    RunBlock,
}

/// `caller` is the program's method the call is made from, as (owner, name).
pub(super) fn router_call(known: &HashSet<String>, caller: Option<(&str, &str)>, method: &str, argv: &[Val]) -> Option<RouterCall> {
    // devise's `devise_scope` wraps its block in `constraints(lambda { |request| request.env["devise.mapping"] = …; true })`: always true, and the env it sets is read only by devise's controllers, whose mapping is not modeled.
    if known.contains("Devise")
        && caller == Some(("ActionDispatch::Routing::Mapper", "devise_scope"))
        && method == "constraints"
        && matches!(argv, [Val::Proc { .. }])
    {
        return Some(RouterCall::RunBlock);
    }
    None
}

/// What the hooks read off the app once.
#[derive(Default)]
pub(super) struct Facts {
    /// Each model's `devise :a, :b` modules, by class name.
    devise_models: HashMap<String, Vec<String>>,
    /// The models whose `devise` call also passes options (`unlock_strategy: :time`), which set that model's own config.
    devise_configured: HashSet<String>,
}

impl Facts {
    pub(super) fn of(app: &App) -> Self {
        let parents = parents(app);
        let calls = devise_calls(app);
        let mut devise_configured: HashSet<String> = calls
            .iter()
            .filter(|(_, args)| args.iter().any(|a| !matches!(&*a.node, ExprNode::Lit { value: Literal::Sym { .. } })))
            .map(|(class, _)| class.to_string())
            .collect();
        // A subclass's are its parent's too (`class_attribute`): chatwoot's `SuperAdmin < User` declares none of its own.
        for name in parents.keys() {
            if ancestors(&parents, name).any(|a| devise_configured.contains(a)) {
                devise_configured.insert(name.to_string());
            }
        }
        Facts { devise_models: devise_modules(&parents, &calls), devise_configured }
    }

    /// Whether `class_answer` may answer for `class`.
    pub(super) fn answers_for(&self, class: &str) -> bool {
        self.devise_models.contains_key(class.trim_start_matches("::"))
    }
}

/// Each of the app's classes and its superclass: a subclass of a model with no table of its own is a library class (chatwoot's `SuperAdmin < User`).
fn parents(app: &App) -> HashMap<&str, Option<&str>> {
    fn parent(p: &Option<crate::ident::ClassId>) -> Option<&str> {
        p.as_ref().map(|p| p.0.as_str().trim_start_matches("::"))
    }
    app.models
        .iter()
        .map(|m| (m.name.0.as_str(), parent(&m.parent)))
        .chain(app.library_classes.iter().map(|c| (c.name.0.as_str(), parent(&c.parent))))
        .collect()
}

/// A class's superclasses among the app's, nearest first.
fn ancestors<'a>(parents: &'a HashMap<&'a str, Option<&'a str>>, class: &str) -> impl Iterator<Item = &'a str> {
    let mut seen: HashSet<&str> = HashSet::default();
    let mut current = parents.get(class).copied().flatten();
    std::iter::from_fn(move || {
        let name = current.filter(|name| seen.insert(name))?;
        current = parents.get(name).copied().flatten();
        Some(name)
    })
}

/// Each class's `devise :a, :b` call, a model's or a library class's (a subclass with no table of its own), with its arguments.
fn devise_calls(app: &App) -> Vec<(&str, &[Expr])> {
    fn is_devise(e: &Expr) -> Option<&[Expr]> {
        match &*e.node {
            ExprNode::Send { recv: None, method, args, .. } if method.as_str() == "devise" => Some(args.as_slice()),
            _ => None,
        }
    }
    let mut out = Vec::new();
    for model in &app.models {
        for item in &model.body {
            if let crate::dialect::ModelBodyItem::Unknown { expr, .. } = item
                && let Some(args) = is_devise(expr)
            {
                out.push((model.name.0.as_str(), args));
            }
        }
    }
    for class in &app.library_classes {
        for call in &class.unknown_calls {
            if let Some(args) = is_devise(call) {
                out.push((class.name.0.as_str(), args));
            }
        }
    }
    out
}

/// Each class's modules as `Devise::Models#devise` reads them: its superclasses' first, as `devise_modules |=` keeps them.
fn devise_modules(parents: &HashMap<&str, Option<&str>>, calls: &[(&str, &[Expr])]) -> HashMap<String, Vec<String>> {
    let mut own: HashMap<&str, Vec<String>> = HashMap::default();
    for (class, args) in calls {
        let modules = own.entry(class).or_default();
        for arg in args.iter() {
            if let ExprNode::Lit { value: Literal::Sym { value } } = &*arg.node {
                modules.push(value.as_str().to_string());
            }
        }
    }
    let mut out: HashMap<String, Vec<String>> = HashMap::default();
    for name in parents.keys() {
        let mut chain: Vec<&str> = ancestors(parents, name).collect();
        chain.reverse();
        chain.push(name);
        let mut modules: Option<Vec<String>> = None;
        for class in chain {
            if let Some(declared) = own.get(class) {
                modules.get_or_insert_with(Vec::new).extend(declared.iter().cloned());
            }
        }
        if let Some(modules) = modules {
            out.insert(name.to_string(), modules);
        }
    }
    out
}

/// A method of a gem's on the mapper whose routes, when it cannot run, are left out and reported, the rest of the call's still drawn.
pub(super) fn omits_on_failure(known: &HashSet<String>, owner: &str, name: &str) -> Option<&'static str> {
    // devise's OmniAuth callbacks: one route per provider `Devise.omniauth_configs` holds, which an app often sets from ENV at boot (mastodon's CAS, SAML and OIDC).
    (known.contains("Devise") && owner == "ActionDispatch::Routing::Mapper" && name == "devise_omniauth_callback")
        .then_some("devise_for: the OmniAuth callback routes are not drawn: they are one per provider the app configures, which is not known here")
}

/// A statement of a gem's routing method whose routes, when it cannot run, are left out and reported, the rest of the call's still drawn.
pub(super) fn omits_statement_on_failure(known: &HashSet<String>, statement: &Expr) -> Option<&'static str> {
    // devise_token_auth's OmniAuth routes: one of them redirects through a block that builds its URL per request, which no route table holds.
    let ExprNode::If { cond, .. } = &*statement.node else { return None };
    if !known.contains("DeviseTokenAuth") {
        return None;
    }
    (crate::emit::ruby::emit_expr(cond).starts_with("defined?(::OmniAuth)") && crate::emit::ruby::emit_expr(statement).contains("DeviseTokenAuth.omniauth_prefix"))
        .then_some("mount_devise_token_auth_for: the OmniAuth routes are not drawn: one redirects through a block that builds its URL per request")
}

/// What a class outside the library classes (a model) answers that a gem's macro gave it.
/// `all` is `Devise::ALL` as the evaluation holds it, the order `devise` keeps its modules in.
/// `config` reads a setting of the gem's (`Devise.unlock_strategy`).
/// An `Err` stops the evaluation with why: what the gem would answer is not known here.
pub(super) fn class_answer(facts: &Facts, class: &str, method: &str, argv: &[Val], all: Option<&[Val]>, config: &mut dyn FnMut(&str) -> Option<Val>) -> Option<Result<Val, String>> {
    // `Devise::Models#devise`, which Devise extends onto every model: it records the modules as `devise_modules`, sorted by `Devise::ALL`.
    let class = class.trim_start_matches("::");
    let modules = facts.devise_models.get(class)?;
    match (method, argv) {
        // `Devise::Models::Omniauthable::ClassMethods`: the model's providers are Devise's (`config.omniauth :github`) unless its own `devise` call names them.
        ("omniauth_providers", []) if modules.iter().any(|m| m == "omniauthable") && !facts.devise_configured.contains(class) => config("omniauth_providers").map(Ok),
        // `Devise::Models::Lockable::ClassMethods`: the model's `unlock_strategy` is Devise's unless its own `devise` call sets one.
        ("unlock_strategy_enabled?", [Val::Sym(strategy)]) if modules.iter().any(|m| m == "lockable") && !facts.devise_configured.contains(class) => {
            let Val::Sym(set) = config("unlock_strategy")? else { return None };
            Some(Ok(Val::Bool(set == *strategy || set == "both")))
        }
        ("respond_to?", [Val::Sym(name), ..]) if matches!(name.as_str(), "devise" | "devise_modules") => Some(Ok(Val::Bool(true))),
        // Not with a module no `Devise.add_module` the evaluation ran registered (devise-two-factor's `:two_factor_authenticatable`): which routes it adds is that gem's.
        ("devise_modules", []) if all.is_none() => Some(Err("`Devise::ALL`, the modules gems register, is not known here".into())),
        ("devise_modules", []) if all.is_some_and(|all| modules.iter().any(|m| !all.iter().any(|a| matches!(a, Val::Sym(s) if s == m)))) => {
            let unknown: Vec<&str> = modules.iter().filter(|m| !all.is_some_and(|all| all.iter().any(|a| matches!(a, Val::Sym(s) if s == *m)))).map(String::as_str).collect();
            Some(Err(format!("`{class}` declares devise modules no gem read here registers (`:{}`)", unknown.join("`, `:"))))
        }
        ("devise_modules", []) => {
            let mut selected: Vec<String> = Vec::new();
            for m in modules {
                if !selected.contains(m) {
                    selected.push(m.clone());
                }
            }
            let rank = |m: &String| all.and_then(|all| all.iter().position(|a| matches!(a, Val::Sym(s) if s == m))).map_or(-1, |i| i as i64);
            selected.sort_by_key(rank);
            Some(Ok(Val::Array(selected.into_iter().map(Val::Sym).collect())))
        }
        _ => None,
    }
}

/// A statement of a gem's method the evaluation passes over without running it, and that writes nothing it reads.
pub(super) fn skips(known: &HashSet<String>, caller: Option<(&str, &str)>, statement: &Expr) -> bool {
    // `Devise.add_mapping`'s `@@helpers.each { |h| h.define_helpers(mapping) }`: each helper module `class_eval`s the mapping's controller helpers (`authenticate_user!`), which no route reads.
    known.contains("Devise")
        && caller == Some(("Devise", "add_mapping"))
        && matches!(&*statement.node, ExprNode::Send { method, block: Some(_), .. } if method.as_str() == "each")
        && crate::emit::ruby::emit_expr(statement).contains("define_helpers(mapping)")
}

/// A top-level statement that only registers its block for later, so replaying the files runs none of it.
pub(super) fn registers_for_later(statement: &Expr) -> bool {
    // Warden's `Manager.after_set_user { … }` and the other `Warden::Hooks`: each appends its block to a list Warden runs per request.
    let ExprNode::Send { recv: Some(recv), method, block, args, .. } = &*statement.node else { return false };
    let ExprNode::Const { path } = &*recv.node else { return false };
    let written: Vec<&str> = path.iter().map(|s| s.as_str()).filter(|s| !s.is_empty()).collect();
    // `Warden::Strategies.add(:label, StrategyClass)`: Warden subclasses the class it is given and keeps it by label; nothing of it runs as it loads.
    if written == ["Warden", "Strategies"] && method.as_str() == "add" && block.is_none() {
        return matches!(args.as_slice(), [label, class] if matches!(&*label.node, ExprNode::Lit { value: Literal::Sym { .. } }) && matches!(&*class.node, ExprNode::Const { .. }));
    }
    block.is_some()
        && written == ["Warden", "Manager"]
        && matches!(
            method.as_str(),
            "after_set_user" | "after_authentication" | "after_fetch" | "after_failed_fetch" | "before_failure" | "before_logout" | "on_request"
                | "prepend_after_set_user" | "prepend_after_authentication" | "prepend_after_fetch" | "prepend_before_failure" | "prepend_before_logout" | "prepend_on_request"
        )
}

/// What a statement passed over in a gem's method may write, when the gem's own contract bounds it tighter than its calls do.
/// `devise` are the locals that hold `Devise` itself where the statement runs; `writer` answers for a setter
/// Devise defines: `Ok` with the class variable it writes and nothing else (`def self.prefix=(v) @@prefix = v end`), `Err` for any other.
/// `None` leaves the statement to the general analysis.
pub(super) fn confined_writes(
    known: &HashSet<String>,
    caller: Option<(&str, &str)>,
    statement: &Expr,
    devise: &HashSet<crate::ident::Symbol>,
    writer: &dyn Fn(&str) -> Option<Result<String, ()>>,
) -> Option<HashSet<(String, bool, String)>> {
    // `Devise.setup do |config| … end` (`yield self`): what it runs configures Devise. An app's `if ENV[…]` around a setting leaves that setting unsure.
    let is_devise = |p: &[crate::ident::Symbol]| p.iter().map(|s| s.as_str()).filter(|s| !s.is_empty()).eq(["Devise"]);
    let mut devise = devise.clone();
    // The call itself fails only by its block's last statement: each earlier one ran, or was passed over with what it names.
    let mut written = statement;
    let setup_call = match &*statement.node {
        ExprNode::Send { recv: Some(r), method, block: Some(b), .. } if method.as_str() == "setup" && matches!(&*r.node, ExprNode::Const { path } if is_devise(path)) => {
            if let ExprNode::Lambda { params, body, .. } = &*b.node {
                devise.extend(params.iter().cloned());
                written = match &*body.node {
                    ExprNode::Seq { exprs } => exprs.last().unwrap_or(body),
                    _ => body,
                };
            }
            true
        }
        _ => false,
    };
    if !known.contains("Devise") || !(caller == Some(("Devise", "setup")) || setup_call) {
        return None;
    }
    struct Walk<'a> {
        devise: &'a HashSet<crate::ident::Symbol>,
        known: &'a HashSet<String>,
        writer: &'a dyn Fn(&str) -> Option<Result<String, ()>>,
        out: HashSet<(String, bool, String)>,
        general: bool,
        /// Locals the statement itself assigns, from what bounds it (`cas_options = {}`): calls on them reach nothing of the app's.
        fresh: HashSet<crate::ident::Symbol>,
    }
    impl Walk<'_> {
        fn is_devise_recv(&self, r: &Expr) -> bool {
            match &*r.node {
                ExprNode::Var { name, .. } => self.devise.contains(name),
                ExprNode::Const { path } => path.iter().map(|s| s.as_str()).filter(|s| !s.is_empty()).eq(["Devise"]),
                _ => false,
            }
        }
        fn safe(&self, r: &Expr) -> bool {
            match &*r.node {
                ExprNode::Var { name, .. } => self.fresh.contains(name),
                // A framework's (`Rails.configuration.x`, `ENV`), whose methods write none of the program's.
                ExprNode::Const { path } => !self.known.contains(&path.iter().map(|s| s.as_str()).filter(|s| !s.is_empty()).collect::<Vec<_>>().join("::")),
                ExprNode::Send { .. } | ExprNode::Lit { .. } | ExprNode::StringInterp { .. } | ExprNode::Array { .. } | ExprNode::Hash { .. } => true,
                _ => false,
            }
        }
        fn setting(&mut self, name: &str) {
            let tag = match name {
                "omniauth" => "@@omniauth_configs".to_string(),
                "warden" => "@@warden_config_blocks".to_string(),
                // Not by its name where Devise defines the setter: one of its own may keep another (`mailer=` writes `@@mailer_ref`). One defined
                // nowhere read here is a `mattr_accessor` of a gem or an initializer not read (mastodon's `ldap_*`, devise_pam's `pam_*`), as Devise's settings are.
                m if m.ends_with('=') => match (self.writer)(m) {
                    Some(Ok(cvar)) => cvar,
                    Some(Err(())) => "*".into(),
                    None => format!("@@{}", m.trim_end_matches('=')),
                },
                _ => "*".into(),
            };
            self.out.insert(("Devise".to_string(), true, tag));
        }
        fn visit(&mut self, e: &Expr) {
            match &*e.node {
                // Not run as the statement runs: a lambda handed to a setting (`setup: ->(env) { … }`), or the block `config.warden` keeps.
                ExprNode::Lambda { .. } => return,
                ExprNode::Send { recv: None, method, block: Some(_), args, .. } if args.is_empty() && matches!(method.as_str(), "lambda" | "proc") => return,
                ExprNode::Send { recv: Some(r), method, args, block, .. } if self.is_devise_recv(r) => {
                    self.setting(method.as_str());
                    for a in args {
                        self.visit(a);
                    }
                    if let Some(b) = block {
                        self.visit(b);
                    }
                    return;
                }
                ExprNode::Assign { target: LValue::Attr { recv, name }, value } | ExprNode::OpAssign { target: LValue::Attr { recv, name }, value, .. } if self.is_devise_recv(recv) => {
                    self.setting(&format!("{}=", name.as_str()));
                    self.visit(value);
                    return;
                }
                // Not a form whose writes the settings named bound: `config` handed on or aliased, a write to anything else, a call of the app's.
                ExprNode::Var { name, .. } if self.devise.contains(name) => self.general = true,
                ExprNode::MultiAssign { .. } => self.general = true,
                ExprNode::Assign { target: LValue::Var { name, .. }, value } if !name.as_str().starts_with("@@") && !name.as_str().starts_with('$') => {
                    self.visit(value);
                    // Not fresh when it holds what it was handed (`target = Devise`, `t = outer`): calls through it reach the app's.
                    if self.safe(value) {
                        self.fresh.insert(name.clone());
                    } else {
                        self.general = true;
                    }
                    return;
                }
                ExprNode::Assign { target: LValue::Index { recv, .. }, .. } | ExprNode::OpAssign { target: LValue::Index { recv, .. }, .. }
                    if matches!(&*recv.node, ExprNode::Var { name, .. } if self.fresh.contains(name)) => {}
                ExprNode::Assign { target, .. } | ExprNode::OpAssign { target, .. } if !matches!(target, LValue::Var { name, .. } if !name.as_str().starts_with("@@") && !name.as_str().starts_with('$')) => self.general = true,
                ExprNode::Send { recv: None, method, .. } if !matches!(method.as_str(), "lambda" | "proc" | "raise" | "require" | "require_relative" | "format" | "sprintf") => self.general = true,
                ExprNode::Send { recv: Some(r), .. } if !self.safe(r) => self.general = true,
                ExprNode::MethodRef { .. } => self.general = true,
                _ => {}
            }
            // A block attached to a call runs with it (`hosts.each { |h| config.host = h }`): its body is walked, not passed over as a value lambda is.
            if let ExprNode::Send { recv, args, block, .. } = &*e.node {
                if let Some(r) = recv {
                    self.visit(r);
                }
                for a in args {
                    self.visit(a);
                }
                if let Some(b) = block {
                    match &*b.node {
                        // Not the app's objects: what a call on a safe receiver yields (`split(",").map(&:strip)`).
                        ExprNode::Lambda { body, params, rest_param, .. } => {
                            self.fresh.extend(params.iter().chain(rest_param.iter()).cloned());
                            self.visit(body);
                        }
                        _ => self.visit(b),
                    }
                }
                return;
            }
            e.node.for_each_child(&mut |c| self.visit(c));
        }
    }
    let mut walk = Walk { devise: &devise, known, writer, out: HashSet::default(), general: false, fresh: HashSet::default() };
    walk.visit(written);
    (!walk.general).then_some(walk.out)
}
