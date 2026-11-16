//! A load hook an app model fires: `ActiveSupport.run_load_hooks(:name, self)`
//! in the model's class body, and the `ActiveSupport.on_load(:name) do … end`
//! an initializer registered for it.
//!
//! Rails keeps a table of blocks per hook name. `run_load_hooks` walks that
//! table and runs each block as `base.class_eval(&block)`, so a block
//! registered in `config/initializers` declares its `class_attribute`, its
//! `def`s and its scopes on the model, at the point in the class body where
//! the model fires the hook. Initializers run before the app's models load,
//! so that is where a block from `config/initializers` runs. One registered
//! after the class has loaded would run at registration instead, after the
//! whole class body, where a later `def` of the same name in the body would
//! lose to it; a compile splices it at the `run_load_hooks` call either way.
//!
//! None of that was carried: the model's `run_load_hooks` statement was a body
//! item no target lowers, the block stayed in the initializer, and
//! `roundhouse check` was quiet — 0 errors while the emitted app lacked
//! everything the block declares and raised `NoMethodError` on first use. The
//! only trace was the initializer ledger's `unconsumed_initializer` warning,
//! which says the opposite of the truth here.
//!
//! This module is the table. [`gather`] reads the app's `config/initializers`
//! once, before the models, and records every top-level
//! `ActiveSupport.on_load(:name)` block with the shape Rails can `class_eval`;
//! [`splices`] answers a model's class body which blocks its `run_load_hooks`
//! statements run; `model::ingest_model` walks those statements as the model's
//! own, with spans naming the initializer they were written in. A block whose
//! shape cannot be class_eval'd is still recorded, so the model that fires it
//! reports an error naming both rather than dropping it; [`spliced`] answers
//! the initializer ledger, so a hook that did run is not reported unconsumed.
//!
//! Out of scope, and still reported elsewhere or unconsumed: hooks registered
//! in `lib/` or an engine (`on_load_reopen` owns the framework names there),
//! and a hook no model body fires.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use ruby_prism::Node;

use crate::span::Span;
use crate::vfs::Vfs;

use super::util::{constant_id_str, constant_path_of, flatten_statements};
use super::{IngestError, IngestResult, prism, sources};

use crate::ingest::prism_api::PrismLocation;

/// The construct an unspliceable hook is reported as, and the one a statement
/// of a spliced block the model ingest refuses is.
pub(super) const ON_LOAD: &str = "ActiveSupport.on_load";
pub(super) const ON_LOAD_STATEMENT: &str = "ActiveSupport.on_load block statement";

/// One top-level `ActiveSupport.on_load(:name) do … end` of an initializer.
struct Hook {
    /// `:name`, without the colon.
    name: String,
    /// The initializer file, as ingest names it for spans.
    file: String,
    /// The same file relative to the app root: how the initializer ledger
    /// names a statement it reports, so the two can be matched.
    shown: String,
    /// The whole file, parsed again by the model that splices the block (the
    /// block's statements must carry spans into this text).
    text: Rc<Vec<u8>>,
    /// The `ActiveSupport.on_load(…)` call, for a refusal's span.
    span: Span,
    /// What keeps Rails from running the block the way a class body is run,
    /// when something does.
    refusal: Option<String>,
    /// The model whose class body the block was spliced into.
    spliced: Option<String>,
}

thread_local! {
    static HOOKS: RefCell<Vec<Hook>> = const { RefCell::new(Vec::new()) };
}

/// The hooks one app ingest registered, cleared when it is dropped, so a
/// later standalone model ingest on the same thread sees none of them.
pub(super) struct Registry;

impl Drop for Registry {
    fn drop(&mut self) {
        HOOKS.with(|hooks| hooks.borrow_mut().clear());
    }
}

/// Read every `config/initializers/**/*.rb` and record the load hooks it
/// registers. Called once per app ingest, before the models: a model's class
/// body splices the blocks registered for the names it fires. The hooks stay
/// registered while the returned [`Registry`] lives.
pub(super) fn gather<V: Vfs + ?Sized>(vfs: &V, dir: &Path) -> IngestResult<Registry> {
    HOOKS.with(|hooks| hooks.borrow_mut().clear());
    let registry = Registry;
    let initializers = dir.join("config").join("initializers");
    if !vfs.is_dir(&initializers) {
        return Ok(registry);
    }
    for path in super::app::read_rb_files(vfs, &initializers)? {
        let Ok(bytes) = vfs.read(&path) else { continue };
        record_file(&path.display().to_string(), &bytes);
    }
    Ok(registry)
}

/// Record the top-level `ActiveSupport.on_load` blocks of one initializer.
fn record_file(file: &str, bytes: &[u8]) {
    let parsed = prism::parse_silent(bytes);
    // A file that does not parse has no block to read: prism's recovered tree
    // is not what Rails runs, and splicing a piece of it would report the
    // file's own syntax errors again, once per statement.
    let unparsed = parsed.errors().next().is_some();
    let root = parsed.node();
    let Some(program) = root.as_program_node() else { return };
    let text = Rc::new(bytes.to_vec());
    let src = String::from_utf8_lossy(bytes);
    for statement in program.statements().body().iter() {
        let Some(call) = statement.as_call_node() else { continue };
        if !is_on_load(&call) {
            continue;
        }
        let Some(name) = call
            .arguments()
            .and_then(|args| args.arguments().iter().next())
            .and_then(|first| first.as_symbol_node().map(|s| String::from_utf8_lossy(s.unescaped()).into_owned()))
        else {
            // No literal name: no model can fire it, and the ledger keeps
            // reporting the statement, so nothing is silent.
            continue;
        };
        let span = super::util::node_span(&statement, file);
        let refusal = if unparsed {
            Some("is written in a file that does not parse".to_string())
        } else {
            refusal(&call, &src)
        };
        let shown = sources::relative_path(file);
        HOOKS.with(|hooks| {
            hooks.borrow_mut().push(Hook {
                name,
                file: file.to_string(),
                shown,
                text: Rc::clone(&text),
                span,
                refusal,
                spliced: None,
            })
        });
    }
}

/// `Const.on_load(…)` with `Const` reaching `ActiveSupport`.
fn is_on_load(call: &ruby_prism::CallNode<'_>) -> bool {
    constant_id_str(&call.name()) == "on_load"
        && call.receiver().is_some_and(|recv| {
            constant_path_of(&recv).is_some_and(|path| path.join("::").trim_start_matches("::") == "ActiveSupport")
        })
}

/// Why this splice would not run the block as the model's class body: a
/// block parameter it does not bind, or registration options it does not
/// read.
fn refusal(call: &ruby_prism::CallNode<'_>, src: &str) -> Option<String> {
    let args: Vec<Node<'_>> = call.arguments().map(|a| a.arguments().iter().collect()).unwrap_or_default();
    if args.len() > 1 {
        let options = args[1..]
            .iter()
            .map(|arg| {
                let loc = arg.location();
                src[loc.start_offset()..loc.end_offset()].to_string()
            })
            .collect::<Vec<_>>()
            .join(", ");
        return Some(format!("is registered with `{options}`, which this does not read"));
    }
    let block = call.block()?;
    let Some(block) = block.as_block_node() else {
        return Some("is passed a block by reference, so what it would run is not written here".to_string());
    };
    if block.parameters().is_some() {
        return Some(
            "takes a block parameter, which this splice does not bind"
                .to_string(),
        );
    }
    None
}

/// One block a model's class body runs, with what it takes to splice it.
#[derive(Clone)]
pub(super) struct Splice {
    /// The hook name, without its colon.
    pub hook: String,
    /// The initializer file the block was written in.
    pub file: String,
    /// That file's text.
    pub text: Rc<Vec<u8>>,
    /// The `ActiveSupport.on_load(…)` call, for a refusal's span.
    pub span: Span,
    /// Why the block cannot be run as the class body, when it cannot.
    pub refusal: Option<String>,
}

/// The blocks the class body's `run_load_hooks` calls run: for each statement
/// that fires a hook, the blocks registered for that name, in the order the
/// initializers registered them.
pub(super) fn splices(stmts: &[Node<'_>]) -> Vec<(usize, Splice)> {
    let mut out = Vec::new();
    let fired: Vec<(usize, String)> = stmts.iter().enumerate().filter_map(|(at, stmt)| Some((at, fired_by(stmt)?))).collect();
    if fired.is_empty() {
        return out;
    }
    HOOKS.with(|hooks| {
        for (at, name) in fired {
            for hook in hooks.borrow().iter().filter(|h| h.name == name) {
                out.push((
                    at,
                    Splice {
                        hook: hook.name.clone(),
                        file: hook.file.clone(),
                        text: Rc::clone(&hook.text),
                        span: hook.span,
                        refusal: hook.refusal.clone(),
                    },
                ));
            }
        }
    });
    out
}

/// The hook name a statement fires, when it is the shape a model's class body
/// can splice: `ActiveSupport.run_load_hooks(:name, self)`. A base other than
/// `self` (Rails' default is `Object`) is not this model's class body, and a
/// name that is not a literal cannot be looked up at compile time.
pub(super) fn fired_by(stmt: &Node<'_>) -> Option<String> {
    let call = stmt.as_call_node()?;
    if constant_id_str(&call.name()) != "run_load_hooks" || call.block().is_some() {
        return None;
    }
    let recv = call.receiver()?;
    if constant_path_of(&recv).is_none_or(|p| p.join("::").trim_start_matches("::") != "ActiveSupport") {
        return None;
    }
    let args: Vec<Node<'_>> = call.arguments().map(|a| a.arguments().iter().collect()).unwrap_or_default();
    let [name, base] = args.as_slice() else { return None };
    if base.as_self_node().is_none() {
        return None;
    }
    name.as_symbol_node().map(|s| String::from_utf8_lossy(s.unescaped()).into_owned())
}

/// The statements a spliced block contributes to the class body: the block's
/// own, with a no-argument `class_eval do … end` / `class_exec do … end`
/// flattened into the statements it runs.
pub(super) fn block_statements<'a>(root: Node<'a>, at: usize) -> Vec<Node<'a>> {
    let Some(program) = root.as_program_node() else { return Vec::new() };
    let Some(stmt) = program
        .statements()
        .body()
        .iter()
        .find(|stmt| stmt.location().start_offset() == at)
    else {
        return Vec::new();
    };
    let Some(body) = stmt
        .as_call_node()
        .and_then(|call| call.block())
        .and_then(|block| block.as_block_node())
        .and_then(|block| block.body())
    else {
        return Vec::new();
    };
    let mut out = Vec::new();
    flatten_class_evals(body, &mut out);
    out
}

fn flatten_class_evals<'a>(body: Node<'a>, out: &mut Vec<Node<'a>>) {
    for stmt in flatten_statements(body) {
        let inner = stmt
            .as_call_node()
            .filter(|call| {
                call.receiver().is_none()
                    && matches!(constant_id_str(&call.name()), "class_eval" | "class_exec")
                    && call.arguments().is_none()
                    && call
                        .block()
                        .and_then(|b| b.as_block_node())
                        .is_some_and(|b| b.parameters().is_none())
            })
            .and_then(|call| call.block())
            .and_then(|block| block.as_block_node())
            .and_then(|block| block.body());
        match inner {
            Some(inner) => flatten_class_evals(inner, out),
            None => out.push(stmt),
        }
    }
}

/// Record that `splice`'s block was written into `model`'s class body, so the
/// initializer ledger no longer calls the hook unconsumed.
pub(super) fn mark_spliced(splice: &Splice, model: &str) {
    HOOKS.with(|hooks| {
        for hook in hooks.borrow_mut().iter_mut() {
            if hook.file == splice.file && hook.span.start == splice.span.start {
                hook.spliced = Some(model.to_string());
            }
        }
    });
}

/// The model `node`'s block was spliced into, when it was: the initializer
/// ledger asks about the `ActiveSupport.on_load(…)` statement `node` is, in
/// the file `path` names.
pub(super) fn spliced(path: &str, node: &Node<'_>) -> Option<String> {
    let at = node.location().start_offset() as u32;
    HOOKS.with(|hooks| {
        hooks
            .borrow()
            .iter()
            .find(|hook| hook.shown == path && hook.span.start == at)
            .and_then(|hook| hook.spliced.clone())
    })
}

/// The message a refused block is reported with, naming the hook and the model
/// that fires it.
pub(super) fn refusal_message(hook: &str, model: &str, reason: &str) -> String {
    format!(
        "`{model}` runs `ActiveSupport.run_load_hooks(:{hook}, self)`, which runs the `ActiveSupport.on_load(:{hook})` block as `class_eval`; this one {reason}, so nothing it declares is carried into `{model}`"
    )
}

/// The message a refused statement of a spliced block is reported with.
pub(super) fn statement_message(hook: &str, model: &str, detail: &str) -> String {
    format!(
        "`{model}` runs `ActiveSupport.run_load_hooks(:{hook}, self)`, and this statement of the `ActiveSupport.on_load(:{hook})` block is not carried into its class body: {detail}"
    )
}

/// The detail half of an [`IngestError`], for a message that quotes it.
pub(super) fn error_detail(err: &IngestError) -> &str {
    match err {
        IngestError::Unsupported { message, .. } | IngestError::Parse { message, .. } => message,
        IngestError::Io(_) => "io error",
    }
}

/// Register an initializer file's text so its spans resolve, and parse it.
///
/// Silent on a syntax error: the initializer is parsed by the app's other
/// passes too, and the file's own diagnostics are theirs to report — a model
/// re-reporting them once per hook it fires would only repeat them.
pub(super) fn parse_hook_file<'a>(file: &str, text: &'a [u8]) -> ruby_prism::ParseResult<'a> {
    sources::register(file, &String::from_utf8_lossy(text));
    prism::parse_silent(text)
}
