//! A mailer's `default to: -> { @user.email }`. actionmailer 8.1.4 runs a
//! Proc default inside `mail`, on the mailer (`instance_exec`, passing
//! the mailer when the Proc takes one argument), for each header the
//! action's `mail(...)` leaves out; a header the action gives isn't
//! computed. So the Proc sees the ivars the before callbacks and the
//! action set.
//!
//! On the ruby family ([`expand`]) each such Proc becomes a private
//! method of the class that declares it, and the default names a call to
//! it:
//!
//! ```ruby
//! default to: -> { @user.email }
//! # becomes, for `mailer_templates` to merge as any other default
//! default to: __mailer_default_1
//!
//! def __mailer_default_1
//!   @user.email
//! end
//! ```
//!
//! A Proc with other parameters, or a method of the app's named as the
//! generated one would be, stays the reported gap, as do strict targets
//! (`mailer_templates` reports a Proc default it is given).

use crate::app::App;
use crate::dialect::{MethodDef, MethodReceiver};
use crate::expr::{Expr, ExprNode};
use crate::ident::Symbol;

const DEFAULT_PREFIX: &str = "__mailer_default_";

/// Turn each mailer's Proc defaults into calls to generated methods.
pub(crate) fn expand(app: &mut App) {
    let mailers = crate::lower::mailer_class_side::mailer_names(app);
    if mailers.is_empty() {
        return;
    }
    // One count for the app: a subclass's method must not share a name
    // with its parent's, which would override it.
    let mut counter = 0usize;
    for lc in app.library_classes.iter_mut() {
        if !mailers.contains(lc.name.0.as_str()) {
            continue;
        }
        if lc.methods.iter().any(|m| m.name.as_str().starts_with(DEFAULT_PREFIX)) {
            continue;
        }
        let mut generated: Vec<MethodDef> = Vec::new();
        for call in lc.unknown_calls.iter_mut() {
            let ExprNode::Send { recv: None, method, args, .. } = &mut *call.node else { continue };
            if method.as_str() != "default" {
                continue;
            }
            let [hash] = args.as_mut_slice() else { continue };
            let ExprNode::Hash { entries, .. } = &mut *hash.node else { continue };
            for (_, value) in entries.iter_mut() {
                let ExprNode::Lambda { params, rest_param: None, extra_params, block_param: None, body, .. } = &*value.node
                else {
                    continue;
                };
                if params.len() > 1 || !extra_params.is_empty() {
                    continue;
                }
                counter += 1;
                let name = format!("{DEFAULT_PREFIX}{counter}");
                let Some(mut method) = method_shell(lc.name.0.as_str(), &name, params.first()) else { continue };
                method.body = body.clone();
                method.enclosing_class = Some(lc.name.0.clone());
                let span = value.span;
                // A Proc taking one argument is given the mailer.
                let args = if params.is_empty() { Vec::new() } else { vec![Expr::new(span, ExprNode::SelfRef)] };
                *value = Expr::new(
                    span,
                    ExprNode::Send { recv: None, method: Symbol::from(name.as_str()), args, block: None, parenthesized: !params.is_empty() },
                );
                generated.push(method);
            }
        }
        lc.methods.extend(generated);
    }
}

/// A private instance method `name` of `class`, with `param` if given,
/// ingested from source for its shape; its body is the caller's.
fn method_shell(class: &str, name: &str, param: Option<&Symbol>) -> Option<MethodDef> {
    let params = param.map(|p| format!("({})", p.as_str())).unwrap_or_default();
    let text = format!("class {class}\n  private\n\n  def {name}{params}\n    nil\n  end\nend\n");
    let (classes, diags) =
        super::prism::scope(|| super::library_class::ingest_library_classes(text.as_bytes(), "<mailer_defaults>"));
    if !diags.is_empty() {
        return None;
    }
    classes.ok()?.into_iter().flat_map(|c| c.methods).find(|m| m.receiver == MethodReceiver::Instance && m.name.as_str() == name)
}
