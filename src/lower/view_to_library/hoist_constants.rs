//! Hoist a pure view-helper call whose arguments are all literals to a
//! class constant built once per process (see `hoist_pure_helper_constants`).
//!
//! Shared lowering, applied by each emitter whose target renders a view
//! module's `constants`: the Ruby/Spinel emitter (after its helper
//! qualification) and the Kotlin emitter. Every target that pays a
//! layout's `stylesheet_link_tag` / `javascript_importmap_tags` per
//! request can opt in the same way once its emitter renders the constants.

use crate::dialect::LibraryClass;
use crate::expr::{Expr, ExprNode, Literal};
use crate::ident::Symbol;

/// Is this a call on `ActionView::ViewHelpers` (or a bare `ViewHelpers`
/// const)?
pub(crate) fn is_view_helpers_const(e: &Expr) -> bool {
    matches!(&*e.node, ExprNode::Const { path }
        if path.last().map(|s| s.as_str() == "ViewHelpers").unwrap_or(false))
}

/// View helpers a call site can be hoisted OUT of, when every argument is
/// a literal: each is a pure function of its arguments and always returns
/// a freshly built String.
///
/// PURITY IS DECLARED HERE, NOT INFERRED, and the list is grown by
/// measurement rather than by reasoning about what looks pure. Two traps
/// make inference the wrong tool:
///
///   - `csrf_token_hidden_input` takes NO arguments, so "every argument is
///     a literal" is vacuously true of it. Hoisting it would compute one
///     session's token at load and serve it to every reader afterwards.
///     Arity is not evidence of purity.
///   - An empty `EffectSet` on the call is not evidence either: it means
///     the analyzer did not model the callee, not that the callee is pure.
///
/// The "freshly built String" half matters as much as the purity half.
/// `image_path("/already/absolute")` RETURNS ITS ARGUMENT, so hoisting it
/// would put a literal's own String object in a constant where a caller
/// could mutate it; the three below all interpolate into a new String.
///
/// `stylesheet_link_tag` / `javascript_include_tag` map a literal name to
/// a path through the immutable emitted asset table, so they are pure
/// over their literals the way `image_tag`
/// is; `javascript_importmap_tags` is pure over the importmap table,
/// which is the one non-literal argument this list admits (see
/// `HOISTABLE_CONST_READERS`). campfire's layout pays all three on every
/// page — 27 stylesheet links, and the importmap's 92 pins rebuilt as
/// Hashes and rendered to JSON — for a header that never changes
/// between one boot and the next: about 360 allocations per request
/// that the room page's profile put beside the message loop's.
const HOISTABLE_TAG_HELPERS: &[&str] = &[
    "image_tag",
    "content_tag",
    "hidden_field_tag",
    "stylesheet_link_tag",
    "javascript_include_tag",
    "javascript_importmap_tags",
];

/// Argument-less reads of emitted TABLES that a hoistable call may take
/// as an argument and still be a per-process constant: `Importmap.pins`
/// and `Importmap.entry` are `config/importmap.rb` rendered as methods
/// answering literals (see `emit_importmap`), so a call over them is as
/// constant as a call over the literals themselves. Declared, like the
/// helpers: a reader is on this list because its body was read, not
/// because it takes no arguments.
const HOISTABLE_CONST_READERS: &[(&str, &str)] = &[("Importmap", "pins"), ("Importmap", "entry")];

/// A canonical spelling of an all-literal expression — `None` the moment
/// any part of it is not a literal. Doubles as the dedupe key, so two call
/// sites spelled the same way share one constant, and as the guard that
/// decides whether a call is hoistable at all.
fn literal_key(e: &Expr) -> Option<String> {
    match &*e.node {
        ExprNode::Lit { value } => Some(match value {
            Literal::Nil => "nil".to_string(),
            Literal::Bool { value } => format!("b:{value}"),
            Literal::Int { value } => format!("i:{value}"),
            Literal::Float { value } => format!("f:{value}"),
            Literal::Str { value } => format!("s:{value:?}"),
            Literal::Sym { value } => format!("y:{}", value.as_str()),
            Literal::Regex { pattern, flags } => format!("r:{pattern:?}/{flags}"),
        }),
        ExprNode::Hash { entries, kwargs } => {
            let mut out = String::from(if *kwargs { "kw{" } else { "h{" });
            for (k, v) in entries {
                out.push_str(&literal_key(k)?);
                out.push_str("=>");
                out.push_str(&literal_key(v)?);
                out.push(',');
            }
            out.push('}');
            Some(out)
        }
        ExprNode::Array { elements, .. } => {
            let mut out = String::from("[");
            for el in elements {
                out.push_str(&literal_key(el)?);
                out.push(',');
            }
            out.push(']');
            Some(out)
        }
        // An already-hoisted constant is a literal for the purposes of
        // the call that takes it (a `+` chain over hoisted tags, below).
        ExprNode::Const { path } if path.len() == 1 && path[0].as_str().starts_with("HOISTED_") => {
            Some(format!("c:{}", path[0].as_str()))
        }
        ExprNode::Send { recv: Some(r), method, args, block: None, .. } if args.is_empty() => {
            let ExprNode::Const { path } = &*r.node else { return None };
            let [konst] = path.as_slice() else { return None };
            HOISTABLE_CONST_READERS
                .iter()
                .any(|(k, m)| *k == konst.as_str() && *m == method.as_str())
                .then(|| format!("{}.{}", konst.as_str(), method.as_str()))
        }
        _ => None,
    }
}

/// A `+` chain whose every leaf is a hoisted constant or a String
/// literal — the layout's `stylesheet_link_tag(…) + "\n" +
/// stylesheet_link_tag(…) + …` once each call is a constant. String `+`
/// is pure and answers a fresh String, so the WHOLE chain is one more
/// constant (its key is the chain's spelling); without this the tags
/// would be hoisted and the 27 concatenations still run per request.
/// Answers the key for a chain, `None` for anything else — including a
/// bare leaf, which is not a chain.
fn hoistable_concat_key(e: &Expr) -> Option<String> {
    fn leaf_or_chain(x: &Expr) -> Option<String> {
        match &*x.node {
            ExprNode::Lit { value: Literal::Str { .. } } => literal_key(x),
            ExprNode::Const { path } if path.len() == 1 && path[0].as_str().starts_with("HOISTED_") => {
                literal_key(x)
            }
            _ => hoistable_concat_key(x),
        }
    }
    let ExprNode::Send { recv: Some(l), method, args, block: None, .. } = &*e.node else {
        return None;
    };
    if method.as_str() != "+" || args.len() != 1 {
        return None;
    }
    Some(format!("plus({},{})", leaf_or_chain(l)?, leaf_or_chain(&args[0])?))
}

/// The dedupe key for a hoistable call, or `None` if this expression is
/// not one: `ActionView::ViewHelpers.<helper>(<literals>)`, no block.
fn hoistable_call_key(e: &Expr) -> Option<String> {
    let ExprNode::Send { recv: Some(r), method, args, block: None, .. } = &*e.node else {
        return None;
    };
    if !is_view_helpers_const(r) || !HOISTABLE_TAG_HELPERS.contains(&method.as_str()) {
        return None;
    }
    let mut key = format!("{}(", method.as_str());
    for a in args {
        key.push_str(&literal_key(a)?);
        key.push(',');
    }
    key.push(')');
    Some(key)
}

/// `image_tag("boost.svg", …)` -> `HOISTED_IMAGE_TAG_BOOST_SVG`. The first
/// String argument names it, because that is what a reader of the emitted
/// view is looking for; a suffix disambiguates two calls that differ only
/// in their options.
fn hoisted_const_name(
    method: &str,
    args: &[Expr],
    used: &mut crate::det::HashSet<String>,
) -> Symbol {
    let mut base = format!("HOISTED_{}", method.to_uppercase());
    if let Some(first) = args.iter().find_map(|a| match &*a.node {
        ExprNode::Lit { value: Literal::Str { value } } => Some(value.clone()),
        _ => None,
    }) {
        let slug: String = first
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_uppercase() } else { '_' })
            .collect();
        let slug = slug.trim_matches('_').to_string();
        if !slug.is_empty() {
            base = format!("{base}_{slug}");
        }
    }
    let mut name = base.clone();
    let mut n = 2;
    while !used.insert(name.clone()) {
        name = format!("{base}_{n}");
        n += 1;
    }
    Symbol::from(name.as_str())
}

/// Replace hoistable calls in one expression tree with references to
/// class constants, minting a constant the first time each distinct call
/// is seen. Children first, so a nested call is hoisted before its parent
/// is examined (a parent that contains one is not itself all-literal, so
/// the two can never both fire on the same node).
fn hoist_in_expr(
    e: &mut Expr,
    minted: &mut crate::det::HashMap<String, Symbol>,
    order: &mut Vec<(Symbol, Expr)>,
    used: &mut crate::det::HashSet<String>,
) {
    hoist_in_expr_at(e, false, minted, order, used)
}

/// `in_concat`: this node is an operand of a `+` the parent will fold
/// if the whole chain qualifies — so a qualifying `+` here is left for
/// the parent, and only the outermost `+` of a chain mints a constant.
fn hoist_in_expr_at(
    e: &mut Expr,
    in_concat: bool,
    minted: &mut crate::det::HashMap<String, Symbol>,
    order: &mut Vec<(Symbol, Expr)>,
    used: &mut crate::det::HashSet<String>,
) {
    let child_in_concat = matches!(&*e.node, ExprNode::Send { method, args, block: None, .. }
        if method.as_str() == "+" && args.len() == 1);
    e.node.for_each_child_mut(&mut |c| hoist_in_expr_at(c, child_in_concat, minted, order, used));
    let (key, label) = match hoistable_call_key(e) {
        Some(key) => (key, None),
        None => match hoistable_concat_key(e) {
            Some(key) => (key, Some("concat")),
            None => return,
        },
    };
    // A chain is hoisted at its TOP: a `+` whose parent is also a `+`
    // over the same kind of leaves is folded by the parent, so the
    // whole chain becomes one constant rather than one per operator.
    if label.is_some() && in_concat {
        return;
    }
    let name = match minted.get(&key) {
        Some(name) => name.clone(),
        None => {
            let ExprNode::Send { method, args, .. } = &*e.node else { unreachable!() };
            let name = match label {
                Some(label) => hoisted_const_name(label, &[], used),
                None => hoisted_const_name(method.as_str(), args, used),
            };
            minted.insert(key, name.clone());
            order.push((name.clone(), e.clone()));
            name
        }
    };
    let span = e.span;
    let ty = e.ty.clone();
    *e = Expr::new(span, ExprNode::Const { path: vec![name] });
    e.ty = ty;
}

/// Hoist a pure view-helper call whose arguments are all literals to a
/// class constant, so the string it always produces is built once at load
/// rather than on every call.
///
/// WHY THIS IS WORTH A PASS. campfire's `_actions.rb` calls `image_tag`
/// seven times per MESSAGE — about 200 times on a 40-message room page —
/// and each call allocates the options hash, the nested `aria:` hash,
/// another hash from `opts.to_h`, the materialised key/value pairs, then
/// an array and a string per attribute in `render_attrs`, to produce a
/// byte-identical `<img>` tag every time. Profiled on the spinel binary,
/// `image_tag` was 13.9% of the whole server's CPU on `/rooms/1`, and
/// `sp_PolyArray_new` was the single hottest leaf underneath it. Hoisting
/// those seven took the page from 206 to 224-238 req/s, byte-identical.
///
/// Hoisting rather than constant-FOLDING is deliberate: computing the
/// string in Rust here would make a second implementation of `image_tag`
/// that can drift from the runtime's, and the emitted constant keeps the
/// one implementation while paying it once per process.
///
/// NAMES ARE UNIQUE ACROSS THE WHOLE SLICE, not per class, and that is not
/// tidiness. campfire's `_actions.rb` and `_template.rb` are two
/// LibraryClasses that both reopen `Views::Messages`, and both ask for the
/// same icon — one with `size: 20` and one without. Named per class they
/// both minted `HOISTED_IMAGE_TAG_MENU_DOTS_HORIZONTAL_SVG`, whichever
/// file loaded second silently won, and every message rendered its
/// options menu with an `<img>` that had lost its `width` and `height`.
/// `scripts/campfire-compare` caught it against Rails; nothing else would
/// have. Dedupe stays PER CLASS (a class must define what it references,
/// since sibling files of one namespace have no load-order contract), so
/// two files hoisting the identical call get two constants with equal
/// values — redundant, and the only alternative that is safe.
pub(crate) fn hoist_pure_helper_constants(lcs: &mut [LibraryClass]) {
    // Seeded from every class, so a hoisted name can never collide with an
    // app constant defined in a sibling file of the same namespace either.
    let mut used: crate::det::HashSet<String> = lcs
        .iter()
        .flat_map(|lc| lc.constants.iter().map(|(n, _)| n.as_str().to_string()))
        .collect();
    for lc in lcs.iter_mut() {
        let mut minted: crate::det::HashMap<String, Symbol> =
            crate::det::HashMap::default();
        // Source order of first use, so the emitted constants read down the
        // file in the order the template reaches them.
        let mut order: Vec<(Symbol, Expr)> = Vec::new();
        for m in &mut lc.methods {
            hoist_in_expr(&mut m.body, &mut minted, &mut order, &mut used);
        }
        lc.constants.extend(order);
    }
}
