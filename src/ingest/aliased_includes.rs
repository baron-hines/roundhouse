//! `include Devise::DeprecatedConstantAccessor`, where devise wrote
//! `DeprecatedConstantAccessor = ActiveSupport::Deprecation::DeprecatedConstantAccessor`:
//! Ruby includes the module the constant holds, so the include names that module.

use crate::app::App;
use crate::expr::ExprNode;
use crate::ident::{ClassId, Symbol};

pub(super) fn resolve(app: &mut App) {
    let declared: crate::det::HashSet<String> = app.library_classes.iter().map(|lc| lc.name.0.to_string()).collect();
    let mut aliases: crate::det::HashMap<String, String> = crate::det::HashMap::default();
    for lc in &app.library_classes {
        for (name, value) in &lc.constants {
            if let ExprNode::Const { path } = &*value.node {
                let rooted = path.first().is_some_and(|p| p.as_str().is_empty());
                let written = path.iter().map(|p| p.as_str()).filter(|p| !p.is_empty()).collect::<Vec<_>>().join("::");
                // Not the name as written: Ruby looks `Inner` up under the namespaces the assignment stands in first.
                let mut target = written.clone();
                if !rooted {
                    let mut scope = Some(lc.name.0.as_str());
                    while let Some(s) = scope {
                        let candidate = format!("{s}::{written}");
                        if declared.contains(&candidate) {
                            target = candidate;
                            break;
                        }
                        scope = s.rsplit_once("::").map(|(outer, _)| outer);
                    }
                }
                aliases.insert(format!("{}::{}", lc.name.0, name), target);
            }
        }
    }
    if aliases.is_empty() {
        return;
    }
    for lc in &mut app.library_classes {
        let owner = lc.name.0.to_string();
        for included in &mut lc.includes {
            let written = included.0.to_string();
            // Not only the name as written: a bare `include Accessor` in `Shelf::Box` reads `Shelf::Accessor` first.
            let mut name = written.clone();
            let mut scope = Some(owner.as_str());
            while let Some(s) = scope {
                let candidate = format!("{s}::{written}");
                if declared.contains(&candidate) {
                    break;
                }
                if aliases.contains_key(&candidate) {
                    name = candidate;
                    break;
                }
                scope = s.rsplit_once("::").map(|(outer, _)| outer);
            }
            let mut hops = 0;
            // Not a name a class or module of the tree declares: that declaration is what Ruby finds first.
            while !declared.contains(&name) && hops < 8 {
                let Some(target) = aliases.get(&name) else { break };
                name = target.clone();
                hops += 1;
            }
            if hops > 0 {
                *included = ClassId(Symbol::from(name.as_str()));
            }
        }
    }
}
