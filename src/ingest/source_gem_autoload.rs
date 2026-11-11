//! A source gem's `Mod.autoload(name, path)` (or `Mod.send(:autoload, …)`) with a computed path, which a method of
//! the gem runs (Devise's `add_module`: `Devise::Models.send(:autoload, camelized.to_sym, path)`).
//!
//! The gem's files are read from source, and each constant the program names is the class or module they define,
//! resolved when it is compiled: the registration that would load the file on the constant's first use has nothing
//! left to load, so it answers its nil. A program that asks `autoload?` or `const_defined?` before that use sees the
//! difference, as Spinel's own literal-path autoload does (the file is loaded when the program starts).
//!
//! Spinel's tree only: the ruby family registers the autoload as written, and Ruby loads the gem's file program from
//! its `$LOAD_PATH` (Devise's model modules, whose files register Warden's strategies).

use crate::App;
use crate::expr::{Expr, ExprNode, Literal};

// Not `sources::path_of`: that table is the ingest's, and this runs on the analyzed App, which keeps its own.
pub(crate) fn answer_nil(app: &mut App) {
    let gem_files: crate::det::HashSet<crate::span::FileId> = app
        .library_classes
        .iter()
        .flat_map(|c| c.methods.iter().map(|m| m.name_span.file))
        .filter(|file| app.source_path(*file).is_some_and(super::absence_guards::is_source_gem_path))
        .collect();
    for class in app.library_classes.iter_mut() {
        for method in class.methods.iter_mut().filter(|m| gem_files.contains(&m.name_span.file)) {
            rewrite(&mut method.body);
        }
    }
}

fn rewrite(e: &mut Expr) {
    if is_receiver_autoload(e) {
        *e.node = ExprNode::Lit { value: Literal::Nil };
        return;
    }
    e.node.for_each_child_mut(&mut |c| rewrite(c));
}

fn is_receiver_autoload(e: &Expr) -> bool {
    let ExprNode::Send { recv: Some(r), method, args, block: None, .. } = &*e.node else { return false };
    // Not a bare `autoload :X, "path"`: Spinel compiles a literal path as a require of its file.
    if !matches!(&*r.node, ExprNode::Const { .. }) {
        return false;
    }
    let args = match method.as_str() {
        "autoload" => args.as_slice(),
        "send" | "__send__" | "public_send" => match args.split_first() {
            Some((name, rest)) if matches!(&*name.node, ExprNode::Lit { value: Literal::Sym { value } } if value.as_str() == "autoload") => rest,
            _ => return false,
        },
        _ => return false,
    };
    // Not a literal path: that file is the program's to load, and its registration stays (only a computed one names no file).
    args.len() == 2
        && args.iter().all(|a| !matches!(&*a.node, ExprNode::Splat { .. }))
        && !matches!(&*args[1].node, ExprNode::Lit { value: Literal::Str { .. } })
}
