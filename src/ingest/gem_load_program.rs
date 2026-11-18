//! What a gem file does as it loads, on the tree that compiles the gem
//! whole (Spinel).
//!
//! The ruby family loads each gem file's program where it is required
//! (`project::apply_file_programs`), and CRuby a source gem's real files.
//! Spinel compiles every class up front and loads no file, so what a
//! file runs beyond its declarations never ran there: devise registers
//! its warden strategy (`Warden::Strategies.add(:database_authenticatable,
//! …)`) and hooks (`Warden::Manager.after_set_user do … end`) so.
//!
//! The statements of the programs the boot loads join the head of the
//! boot program, in the order Ruby loads their files: each boot gem entry
//! as Bundler requires it, a required file's statements ahead of the
//! lines after its `require`; then, as Ruby loads them on a constant's
//! first read, the files declaring a constant the app's own code names
//! and the files an `autoload` of such a constant names. Devise builds
//! its `Devise::Models.autoload`s at run time; the model's `devise
//! :database_authenticatable` reads the module, whose file requires the
//! strategy's.
//! [`App::gem_load_statements`] names them, so the ruby family leaves
//! them to the programs.
//!
//! Taken: a call on a class the tree carries. Any other statement such a
//! file runs, and one in a file-level `if`, is reported where it stands:
//! the tree does not run it.

use crate::app::{App, FileProgram, ProgramItem};
use crate::det::{HashMap, HashSet};
use crate::expr::{Expr, ExprNode};

pub(super) fn carry(app: &mut App) {
    // (not a class's own `class_eval`: it may never run the block)
    let evaluators: HashSet<String> = app.library_classes.iter()
        .filter(|c| c.methods.iter().any(|m| m.receiver == crate::dialect::MethodReceiver::Class
            && matches!(m.name.as_str(), "class_eval" | "module_eval" | "class_exec" | "module_exec")))
        .map(|c| c.name.0.to_string())
        .collect();
    for program in &mut app.file_programs {
        let items = std::mem::take(&mut program.items);
        for item in items {
            match &item {
                ProgramItem::Statement(e) => match class_eval_mixins(e, &evaluators) {
                    Some(calls) => program.items.extend(calls.into_iter().map(ProgramItem::Statement)),
                    None => program.items.push(item),
                },
                _ => program.items.push(item),
            }
        }
    }
    // `Foo::Version.extend(VersionGem::Basic)` where Basic has an `extended` hook: Spinel takes the call as Foo's
    // extend but does not run the hook there, so what the hook mixes in (version_gem's Api) never reaches Foo.
    let hooked: HashSet<String> = app.library_classes.iter()
        .filter(|c| c.methods.iter().any(|m| m.receiver == crate::dialect::MethodReceiver::Class && m.name.as_str() == "extended"))
        .map(|c| c.name.0.to_string())
        .collect();
    for program in &app.file_programs {
        for item in &program.items {
            let ProgramItem::Statement(e) = item else { continue };
            let ExprNode::Send { recv: Some(recv), method, args, .. } = &*e.node else { continue };
            if method.as_str() != "extend" || !matches!(&*recv.node, ExprNode::Const { .. }) { continue; }
            for arg in args {
                let ExprNode::Const { path } = &*arg.node else { continue };
                let name = path.iter().map(|p| p.as_str()).filter(|p| !p.is_empty()).collect::<Vec<_>>().join("::");
                if hooked.contains(&name) {
                    app.gem_route_gaps.push(crate::diagnostic::Diagnostic::unsupported(
                        e.span,
                        Some(crate::ident::Symbol::from("spinel")),
                        "mixin hook",
                        format!("Spinel does not run `{name}.extended` for a file-level `extend`, so what the hook mixes in is missing there"),
                    ));
                }
            }
        }
    }
    let programs: HashMap<String, &FileProgram> =
        app.file_programs.iter().map(|p| (super::normalize_source(&p.source), p)).collect();
    if programs.is_empty() {
        return;
    }
    let known: HashSet<String> = app.sources.iter().map(|f| super::normalize_source(&f.path)).collect();
    let classes: HashSet<&str> = app.library_classes.iter().map(|c| c.name.0.as_str()).collect();
    let named = named_by_the_app(app, &programs);
    let mut walk = Walk { programs: &programs, known: &known, classes: &classes, named: &named, seen: HashSet::default(), lazy: Vec::new(), taken: Vec::new(), left: Vec::new() };
    let mut roots: Vec<String> = app.boot_gem_entries.iter().map(|e| super::normalize_source(e)).collect();
    roots.reverse();
    let mut by_path: Vec<&String> = programs.keys().collect();
    by_path.sort();
    walk.lazy.extend(by_path.into_iter().filter(|path| {
        programs[*path].items.iter().any(|item| match item {
            ProgramItem::Declared(names) => names.iter().any(|n| named.contains(n.0.as_str())),
            _ => false,
        })
    }).cloned());
    while let Some(next) = roots.pop().or_else(|| (!walk.lazy.is_empty()).then(|| walk.lazy.remove(0))) {
        walk.file(&next);
    }
    let (taken, left) = (walk.taken, walk.left);
    app.gem_loaded_files.extend(walk.seen);
    app.gem_load_statements.extend(taken.iter().map(|e| e.span));
    app.boot_program.splice(0..0, taken);
    app.gem_route_gaps.extend(left.into_iter().map(|span| {
        let mut gap = crate::diagnostic::Diagnostic::unsupported(
            span,
            Some(crate::ident::Symbol::from("spinel")),
            "gem file statement",
            "a gem file's statement as it loads runs on this tree only as a call on a class it carries".to_string(),
        );
        gap.severity = crate::diagnostic::Severity::Warning;
        gap
    }));
}

/// `VersionGem::Version.class_eval do extend VersionGem::Basic end` (version_gem, snaky_hash): a file-level
/// block of nothing but mixins, as the calls `VersionGem::Version.extend(VersionGem::Basic)` it runs. Both read
/// their constants from the file's top level, so the calls name what the block's statements named.
fn class_eval_mixins(e: &Expr, evaluators: &HashSet<String>) -> Option<Vec<Expr>> {
    let ExprNode::Send { recv: Some(recv), method, args, block: Some(block), .. } = &*e.node else { return None };
    let ExprNode::Const { path } = &*recv.node else { return None };
    let written = path.iter().map(|p| p.as_str()).filter(|p| !p.is_empty()).collect::<Vec<_>>().join("::");
    if evaluators.contains(&written)
        || !matches!(method.as_str(), "class_eval" | "module_eval" | "class_exec" | "module_exec")
        || !args.is_empty()
    {
        return None;
    }
    let ExprNode::Lambda { params, rest_param: None, extra_params, block_param: None, body, .. } = &*block.node else { return None };
    if !params.is_empty() || !extra_params.is_empty() {
        return None;
    }
    let statements: Vec<&Expr> = match &*body.node {
        ExprNode::Seq { exprs } => exprs.iter().collect(),
        _ => vec![body],
    };
    let mut calls = Vec::new();
    for statement in statements {
        let ExprNode::Send { recv: None, method, args, block: None, .. } = &*statement.node else { return None };
        if !matches!(method.as_str(), "extend" | "include" | "prepend")
            || args.is_empty()
            || !args.iter().all(|a| matches!(&*a.node, ExprNode::Const { .. }))
        {
            return None;
        }
        calls.push(Expr::new(statement.span, ExprNode::Send {
            recv: Some(recv.clone()),
            method: method.clone(),
            args: args.clone(),
            block: None,
            parenthesized: true,
        }));
    }
    (!calls.is_empty()).then_some(calls)
}

/// The constants the app's own code (any body outside the gem files' programs) and its model macros' includes name.
fn named_by_the_app(app: &App, programs: &HashMap<String, &FileProgram>) -> HashSet<String> {
    let files: HashSet<crate::span::FileId> = app
        .sources
        .iter()
        .enumerate()
        .filter(|(_, f)| programs.contains_key(&super::normalize_source(&f.path)))
        .map(|(i, _)| crate::span::FileId(i as u32 + 1))
        .collect();
    let mut named: HashSet<String> = app.macro_includes.iter().map(|(_, m)| m.0.to_string()).collect();
    crate::lower::for_each_emit_body_ref(app, &mut |body| {
        if files.contains(&body.span.file) {
            return;
        }
        visit(body, &mut |e| {
            // Each enclosing name too: Ruby reads `Foo` and `Foo::Bar` on its way to `Foo::Bar::Baz`.
            if let ExprNode::Const { path } = &*e.node {
                let segments: Vec<&str> = path.iter().map(|s| s.as_str()).filter(|s| !s.is_empty()).collect();
                for end in 1..=segments.len() {
                    named.insert(segments[..end].join("::"));
                }
            }
        });
    });
    named
}

fn visit(e: &Expr, f: &mut dyn FnMut(&Expr)) {
    f(e);
    e.node.for_each_child(&mut |c| visit(c, f));
}

struct Walk<'a> {
    programs: &'a HashMap<String, &'a FileProgram>,
    known: &'a HashSet<String>,
    classes: &'a HashSet<&'a str>,
    named: &'a HashSet<String>,
    seen: HashSet<String>,
    lazy: Vec<String>,
    taken: Vec<Expr>,
    left: Vec<crate::span::Span>,
}

impl Walk<'_> {
    fn file(&mut self, path: &str) {
        let Some(program) = self.programs.get(path).copied() else { return };
        if !self.seen.insert(path.to_string()) {
            return;
        }
        for item in &program.items {
            match item {
                ProgramItem::Require { feature, relative, .. } => {
                    if let Some(target) = super::resolve_gem_require(self.known, path, feature, *relative) {
                        self.file(&target);
                    }
                }
                ProgramItem::Autoload { owner, name, feature, .. } => {
                    let constant = if owner == "Object" { name.clone() } else { format!("{owner}::{name}") };
                    if !self.named.contains(&constant) {
                        continue;
                    }
                    if let Some(target) = super::resolve_gem_require(self.known, path, feature, false) {
                        self.lazy.push(target);
                    }
                }
                ProgramItem::Statement(e) if self.carried_call(e) => self.taken.push(e.clone()),
                ProgramItem::Statement(e) => self.left.push(e.span),
                ProgramItem::NotCarried { span, .. } => self.left.push(*span),
                ProgramItem::Branch { then_items, else_items, .. } => {
                    let mut pending: Vec<&ProgramItem> = then_items.iter().chain(else_items).collect();
                    while let Some(nested) = pending.pop() {
                        match nested {
                            ProgramItem::Statement(e) => self.left.push(e.span),
                            ProgramItem::NotCarried { span, .. } => self.left.push(*span),
                            ProgramItem::Branch { then_items, else_items, .. } => pending.extend(then_items.iter().chain(else_items)),
                            _ => {}
                        }
                    }
                }
                ProgramItem::Declared(_) | ProgramItem::Opened(_) => {}
            }
        }
    }

    fn carried_call(&self, e: &Expr) -> bool {
        let ExprNode::Send { recv: Some(r), .. } = &*e.node else { return false };
        let ExprNode::Const { path } = &*r.node else { return false };
        let name = path.iter().map(|s| s.as_str()).filter(|s| !s.is_empty()).collect::<Vec<_>>().join("::");
        self.classes.contains(name.as_str())
    }
}
