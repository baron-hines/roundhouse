//! A has_one's writer from app code (`owner.profile = record`). Rails 8.1.4's
//! `HasOneAssociation#replace` holds the record as the association's
//! target and, on a persisted owner, writes it at once:
//! - the old target is removed per `dependent:` (`:destroy` destroys it,
//!   `:delete` deletes it, otherwise its key is nulled and it is saved,
//!   and a failed save restores the key and raises `RecordNotSaved`);
//! - the new one gets the owner's key (and `as:` type, the owner's
//!   polymorphic name), its `belongs_to` inverse is set to the owner, and
//!   it is saved, raising `RecordNotSaved` when the save fails.
//!
//! The removal and the save are one transaction, so a failed save
//! leaves the old record as it was, in the table and as the target.
//!
//! On a new owner the record is held, with its inverse set, and saved
//! with the owner's key after the owner saves (`save_has_one_association`).
//!
//! On the ruby family, each direct has_one that app code assigns, by
//! name, gets this model source after its declaration:
//!
//! ```ruby
//! def profile=(record)            # Rails' replace
//! def __written_profile_save      # after_save: a held record saved with the key
//! ```
//!
//! The reader already answers `@profile_cache` once `@profile_loaded`.
//! A `validate: true` has_one gets `__validate_<name>_target`, whatever
//! app code assigns; the strict targets keep the option the warned gap
//! (`refuse`).
//!
//! An `autosave: true` has_one keeps the writer and after_save the shared
//! lowerer synthesizes (`model_to_library::associations`), and a model
//! that defines the writer itself keeps its own.

use std::collections::BTreeSet;

use crate::app::App;
use crate::dialect::{Association, Dependent, ModelBodyItem};
use crate::expr::{Expr, ExprNode, LValue};

/// The names app code (seeds included) assigns through a writer
/// (`x.profile = v`).
pub(super) fn assigned_names(app: &App) -> BTreeSet<String> {
    fn walk(e: &Expr, out: &mut BTreeSet<String>) {
        match &*e.node {
            ExprNode::Assign { target: LValue::Attr { name, .. }, .. } => {
                out.insert(name.as_str().trim_end_matches('=').to_string());
            }
            ExprNode::Send { recv: Some(_), method, args, .. }
                if args.len() == 1
                    && method.as_str().ends_with('=')
                    && !matches!(method.as_str(), "==" | "!=" | "<=" | ">=" | "===" | "[]=") =>
            {
                out.insert(method.as_str().trim_end_matches('=').to_string());
            }
            _ => {}
        }
        e.node.for_each_child(&mut |c| walk(c, out));
    }
    let mut out = BTreeSet::new();
    for model in &app.models {
        model.methods().for_each(|m| walk(&m.body, &mut out));
    }
    for controller in &app.controllers {
        controller.actions().for_each(|a| walk(&a.body, &mut out));
        controller.class_methods().for_each(|m| walk(&m.body, &mut out));
    }
    for lc in &app.library_classes {
        lc.methods.iter().for_each(|m| walk(&m.body, &mut out));
    }
    for view in &app.views {
        walk(&view.body, &mut out);
    }
    if let Some(seeds) = &app.seeds {
        walk(seeds, &mut out);
    }
    out
}

pub(crate) fn expand(app: &mut App) {
    let assigned = assigned_names(app);
    let snapshot = app.models.clone();
    for model in &mut app.models {
        let mut inserts: Vec<(usize, Vec<ModelBodyItem>)> = Vec::new();
        for (index, item) in model.body.iter().enumerate() {
            let ModelBodyItem::Association { assoc, .. } = item else { continue };
            // `validate: true`: Rails' `validate_single_association` checks
            // the loaded target when it is new or changed, and the owner
            // takes `<name> is invalid`.
            if let Association::HasOne { name, validate: true, through: None, .. } = assoc {
                let n = name.as_str();
                let src = format!(
                    "class Generated\n  def __validate_{n}_target\n    return unless @{n}_loaded\n    record = @{n}_cache\n    return if record.nil?\n    return unless record.new_record? || record.changed?\n    errors.add(:{n}, :invalid) unless record.valid?\n  end\n\n  validate :__validate_{n}_target\nend\n"
                );
                if let Some(items) = super::validates_with::ingest_source(&src, &model.name) {
                    inserts.push((index + 1, items));
                }
            }
            let Association::HasOne {
                name,
                target,
                foreign_key,
                dependent,
                through: None,
                as_interface,
                autosave: false,
                primary_key: None,
                ..
            } = assoc
            else {
                continue;
            };
            // `destroy_async` removes the old target through a job, which
            // isn't modeled here.
            if !assigned.contains(name.as_str()) || matches!(dependent, Dependent::DestroyAsync) {
                continue;
            }
            // Only an app model: a gem's has no `save` here to call.
            let Some(target_model) = snapshot.iter().find(|m| &m.name == target) else { continue };
            let n = name.as_str();
            if model.methods().any(|m| m.name.as_str() == format!("{n}=")) {
                continue;
            }
            // The target's `belongs_to` on the same key: Rails sets it to the
            // owner (the inverse instance), so the record reads its owner
            // without a query and a required one holds before the save.
            let inverse = target_model.associations().find_map(|a| match a {
                Association::BelongsTo { name: bt, foreign_key: fk, polymorphic, .. }
                    if fk == foreign_key
                        && (*polymorphic == as_interface.is_some())
                        && as_interface.as_ref().is_none_or(|i| i == bt) =>
                {
                    Some(bt.as_str().to_string())
                }
                _ => None,
            });
            let fk = foreign_key.as_str();
            // Rails writes `polymorphic_name`, the base class's name, which
            // for a has_one declared on a model is the model itself.
            let owner = model.name.0.as_str();
            let set_type = |var: &str, value: &str| {
                as_interface.as_ref().map(|i| format!("{var}.{i}_type = {value}\n")).unwrap_or_default()
            };
            let set_inverse = match &inverse {
                // The inverse's writer also writes the key (and type), which
                // are then written again below, as Rails' set_owner_attributes
                // does after it.
                Some(bt) => format!("        record.{bt} = self\n"),
                None => String::new(),
            };
            // A failed save of the new record restores, in memory, what the
            // rolled-back transaction restores in the table: the old
            // record's key (Rails' `set_owner_attributes(target)`), and the
            // new one's nulled (`nullify_owner_attributes`).
            let restore_old = match dependent {
                Dependent::Destroy | Dependent::Delete => String::new(),
                _ => format!(
                    "            unless old.nil?\n              old.{fk} = id\n{}            end\n",
                    set_type("              old", &format!("\"{}\"", model.name.0.as_str())),
                ),
            };
            let remove_old = match dependent {
                Dependent::Destroy => "          old.destroy\n".to_string(),
                Dependent::Delete => "          old.delete\n".to_string(),
                // Rails' `remove_target!`: a save the nulled key fails (a
                // required `belongs_to`) restores the key and raises.
                _ => format!(
                    "          old.{fk} = nil\n{}          unless old.save\n            old.{fk} = id\n{}            raise ActiveRecord::RecordNotSaved, \"Failed to remove the existing associated {n}. The record failed to save after its foreign key was set to nil.\"\n          end\n",
                    set_type("          old", "nil"),
                    set_type("            old", &format!("\"{}\"", model.name.0.as_str())),
                    n = name.as_str(),
                ),
            };
            let src = format!(
                "class Generated\n  def {n}=(record)\n    old = {n}\n    unless old.equal?(record)\n      if persisted?\n        {owner}.transaction do\n        unless old.nil?\n{remove_old}        end\n        unless record.nil?\n{set_inverse}          record.{fk} = id\n{set_type_rec}          unless record.save\n            record.{fk} = nil\n{set_type_nil}{restore_old}            raise ActiveRecord::RecordNotSaved, \"Failed to save the new associated {n}.\"\n          end\n        end\n        end\n      else\n        unless record.nil?\n{set_inverse}{set_type_rec}        end\n        @__written_{n} = true\n      end\n    end\n    @{n}_cache = record\n    @{n}_loaded = true\n    record\n  end\n\n  def __written_{n}_save\n    return unless @__written_{n}\n    @__written_{n} = false\n    record = @{n}_cache\n    return if record.nil?\n    record.{fk} = id\n{set_type_rec}    record.save\n  end\n\n  after_save :__written_{n}_save\nend\n",
                set_type_rec = set_type("          record", &format!("\"{owner}\"")),
                set_type_nil = set_type("            record", "nil"),
            );
            if let Some(mut items) = super::validates_with::ingest_source(&src, &model.name) {
                // The association's own accessor, not an app def overriding it
                // (`diagnostics::overridden_synthesized_accessors`).
                for item in &mut items {
                    if let ModelBodyItem::Method { method, .. } = item {
                        method.name_span = crate::span::Span::synthetic();
                    }
                }
                inserts.push((index + 1, items));
            }
        }
        for (at, items) in inserts.into_iter().rev() {
            for (k, item) in items.into_iter().enumerate() {
                model.body.insert(at + k, item);
            }
        }
    }
}

/// `validate: true` where [`expand`] doesn't validate the target stays the
/// reported gap it was before `HasOne::validate`: on a target the generated
/// source doesn't reach (`lowered` false, the strict targets), and on a
/// `through:` has_one anywhere.
pub(crate) fn refuse(app: &App, lowered: bool) -> Vec<crate::diagnostic::Diagnostic> {
    let mut out = Vec::new();
    for model in &app.models {
        for item in &model.body {
            let ModelBodyItem::Association { assoc: Association::HasOne { name, validate: true, through, .. }, .. } =
                item
            else {
                continue;
            };
            if lowered && through.is_none() {
                continue;
            }
            let mut diagnostic = crate::diagnostic::Diagnostic::unsupported(
                item.span(),
                None,
                format!("has_one :{}, validate:", name.as_str()),
                "this association option is not modelled and is ignored",
            );
            diagnostic.severity = crate::diagnostic::Severity::Warning;
            out.push(diagnostic);
        }
    }
    out
}
