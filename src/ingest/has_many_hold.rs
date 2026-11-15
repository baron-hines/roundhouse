//! A has_many's `build` and `<<` from app code (#1364). Rails 8.1.4 keeps a
//! record built on, or pushed onto, an owner in the association's target:
//! the owner validates it with itself (a has_many's `validate: true`
//! default: "Tasks is invalid") and saves it after itself, with the
//! foreign key. `<<` on a persisted owner sets the key and saves the
//! record at once (an invalid one stays unsaved); on a new owner it is
//! held like a built one. A new owner's reader answers the records it
//! holds.
//!
//! On the ruby family ([`expand`]), each has_many that app code builds on
//! or pushes onto (`<name>.build` / `<name> << record`, by name) gets
//! model source, after its declaration:
//!
//! ```ruby
//! def __hold_tasks(record)        # a lowered `tasks.build(…)` passes its record
//! def __push_tasks(record)        # a lowered `tasks << record`
//! validate :__held_tasks_valid    # each held record valid, else `errors.add(:tasks, :invalid)`
//! after_save :__held_tasks_save   # each held record saved, with the key
//! after_save_commit :__held_tasks_committed  # the held list cleared once the save commits
//! def __tasks_first(rel)          # and _last, _size, _length, _empty?, _any?, _destroy_all
//! def tasks=(records)             # app code assigns it: Rails' replace
//! ```
//!
//! Rails answers a collection's `first`/`last` from its loaded target
//! when unsaved records are in it (`find_from_target?`), adds them to
//! `size` (`unsaved_records.size + count_records`), and destroys them
//! with the rest in `destroy_all`. The terminals take the seeded relation
//! and answer the held records that are still new along with it;
//! `count` and the finders stay the query's, as Rails' do. A new owner's
//! target is the held list alone (Rails doesn't query for an unsaved
//! owner), and `destroy_all` leaves the association unloaded.
//!
//! The writer is Rails' `replace`: on a new owner the records are held,
//! as pushed ones are. On a persisted owner, in a transaction, each
//! current record not among them is removed per `dependent:` (destroyed,
//! deleted, or else its key nulled in the table without callbacks, as
//! Rails' nullify is, the loaded record left as it was), and each new one is pushed (key set, saved), raising
//! `RecordNotSaved` when a save fails. A `dependent: :destroy` removal
//! is `destroy!`, so a callback's abort raises and rolls it all back. A
//! built record is already in the target: one the list keeps stays held
//! and unsaved, and one it leaves out is dropped. The association is then
//! loaded with the records, held ones included, so a read enumerates them;
//! the terminals add only the held records the loaded target doesn't
//! already have. A record pushed onto a persisted owner is saved and the
//! association unloaded, so the next read sees it. A scoped has_many
//! keeps no writer here.
//!
//! `lower::scope_chain` routes the calls there, and the reader's unsaved
//! guard answers the held records, for a model that has them.

use std::collections::BTreeSet;

use crate::app::App;
use crate::dialect::{Association, Dependent, ModelBodyItem};
use crate::expr::{Expr, ExprNode};

/// The has_many names app code builds on or pushes onto.
fn used_names(app: &App) -> BTreeSet<String> {
    fn walk(e: &Expr, out: &mut BTreeSet<String>) {
        if let ExprNode::Send { recv: Some(recv), method, .. } = &*e.node
            && matches!(method.as_str(), "build" | "<<")
            && let ExprNode::Send { method: name, args, .. } = &*recv.node
            && args.is_empty()
        {
            out.insert(name.as_str().to_string());
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
    out
}

pub(crate) fn expand(app: &mut App) {
    // A has_many app code assigns (`owner.tasks = records`) holds too: a
    // new owner holds what it's handed, and the writer pushes through
    // `__push_<name>`.
    let assigned = super::has_one_write::assigned_names(app);
    let mut used = used_names(app);
    used.extend(assigned.iter().cloned());
    if used.is_empty() {
        return;
    }
    let snapshot = app.models.clone();
    // The generated methods' signatures: their calls are written after
    // analysis (`lower::scope_chain`), so no call site types `record`.
    let mut signatures: Vec<(crate::ident::ClassId, String, crate::ident::ClassId)> = Vec::new();
    for model in &mut app.models {
        let mut inserts: Vec<(usize, Vec<ModelBodyItem>)> = Vec::new();
        for (index, item) in model.body.iter().enumerate() {
            let ModelBodyItem::Association { assoc, .. } = item else { continue };
            let Association::HasMany {
                name, target, foreign_key, through: None, as_interface, primary_key: None, dependent, scope, ..
            } = assoc
            else {
                continue;
            };
            if !used.contains(name.as_str()) {
                continue;
            }
            // Only an app model: a gem's (`Doorkeeper::Application`) has no
            // `save` here to call.
            if !snapshot.iter().any(|m| &m.name == target) {
                continue;
            }
            let hold = format!("__hold_{}", name.as_str());
            if model.methods().any(|m| m.name.as_str().starts_with(&hold) || m.name.as_str() == format!("__push_{name}")) {
                continue;
            }
            // The target's `belongs_to` on the same key, set to the owner as
            // Rails sets the inverse, so a required one holds before the save.
            let inverse = snapshot.iter().find(|m| &m.name == target).and_then(|t| {
                t.associations().find_map(|a| match a {
                    Association::BelongsTo { name: bt, foreign_key: fk, polymorphic: false, .. } if fk == foreign_key => {
                        Some(bt.as_str().to_string())
                    }
                    _ => None,
                })
            });
            // A held child is checked as Rails checks it with the inverse
            // set: its `belongs_to`'s "must exist" reads the key, which the
            // save sets, so that error alone doesn't make it invalid.
            let counts = match &inverse {
                Some(bt) => format!("      child.errors.each {{ |error| invalid = true unless error.attribute.to_s == \"{bt}\" }}\n"),
                None => "      invalid = true\n".to_string(),
            };
            let set_owner = match &inverse {
                Some(bt) => format!("      record.{bt} = self\n"),
                None => String::new(),
            };
            let set_type = as_interface
                .as_ref()
                .map(|i| format!("      record.{i}_type = \"{}\"\n", model.name.0.as_str()))
                .unwrap_or_default();
            let n = name.as_str();
            let fk = foreign_key.as_str();
            // The model by name, not `self.class`: Spinel loses a local a
            // block assigns when the block raises out of a `self.class` call
            // (matz/spinel#10291).
            let owner = model.name.0.as_str();
            let src = format!(
                "class Generated\n  def __hold_{n}(record)\n    @__held_{n} = [] if @__held_{n}.nil?\n    @__held_{n} << record\n    record\n  end\n\n  def __push_{n}(record)\n    if new_record?\n{set_owner}{set_type}      __hold_{n}(record)\n      {n}\n    else\n      record.{fk} = id\n{set_type}      return nil unless record.save\n      @{n}_loaded = false\n      {n}\n    end\n  end\n\n  def __push_all_{n}(records)\n    if new_record?\n      records.each {{ |record| __push_{n}(record) }}\n      return {n}\n    end\n    pushed = true\n    {owner}.transaction do\n      records.each do |record|\n        next unless __push_{n}(record).nil?\n        pushed = false\n        raise ActiveRecord::Rollback\n      end\n    end\n    pushed ? {n} : nil\n  end\n\n  def __held_{n}_list\n    @__held_{n}.nil? ? [] : @__held_{n}\n  end\n\n  def __held_{n}_unsaved\n    unsaved = __held_{n}_list.select {{ |record| record.new_record? }}\n    return unsaved unless {n}_loaded?\n    loaded = {n}_target\n    unsaved.reject {{ |record| loaded.any? {{ |c| c.equal?(record) }} }}\n  end\n\n  def __{n}_first(rel)\n    return __held_{n}_list.first if new_record?\n    unsaved = __held_{n}_unsaved\n    return rel.first if unsaved.empty?\n    found = rel.first\n    found.nil? ? unsaved.first : found\n  end\n\n  def __{n}_last(rel)\n    return __held_{n}_list.last if new_record?\n    unsaved = __held_{n}_unsaved\n    unsaved.empty? ? rel.last : unsaved.last\n  end\n\n  def __{n}_size(rel)\n    return __held_{n}_list.size if new_record?\n    rel.size + __held_{n}_unsaved.size\n  end\n\n  def __{n}_length(rel)\n    return __held_{n}_list.size if new_record?\n    rel.length + __held_{n}_unsaved.size\n  end\n\n  def __{n}_empty?(rel)\n    return __held_{n}_list.empty? if new_record?\n    __held_{n}_unsaved.empty? && rel.empty?\n  end\n\n  def __{n}_any?(rel)\n    return !__held_{n}_list.empty? if new_record?\n    !__held_{n}_unsaved.empty? || rel.any?\n  end\n\n  def __{n}_destroy_all(rel)\n    if new_record?\n      destroyed = __held_{n}_list\n      destroyed.each {{ |record| record.destroy }}\n    else\n      unsaved = __held_{n}_unsaved\n      destroyed = rel.destroy_all\n      unsaved.each do |record|\n        record.destroy\n        destroyed << record\n      end\n    end\n    @__held_{n} = nil\n    @{n}_loaded = false\n    destroyed\n  end\n\n  def __held_{n}_valid\n    return if @__held_{n}.nil?\n    invalid = false\n    __held_{n}_list.each do |child|\n      next if child.valid?\n{counts}    end\n    errors.add(:{n}, :invalid) if invalid\n  end\n\n  validate :__held_{n}_valid\n\n  def __held_{n}_save\n    return if @__held_{n}.nil? || @__held_{n}_saving\n    @__held_{n}_saving = true\n    begin\n      __held_{n}_list.each do |child|\n        child.{fk} = id\n        child.save!\n      end\n    ensure\n      @__held_{n}_saving = false\n    end\n  end\n\n  after_save :__held_{n}_save\n\n  def __held_{n}_committed\n    @__held_{n} = nil\n  end\n\n  after_save_commit :__held_{n}_committed\nend\n"
            );
            let writer = format!("{n}=");
            let writes = assigned.contains(n)
                && scope.is_none()
                && !matches!(dependent, Dependent::DestroyAsync)
                && !model.methods().any(|m| m.name.as_str() == writer);
            let src = if writes {
                let remove = match dependent {
                    Dependent::Destroy => "        old.destroy!\n".to_string(),
                    Dependent::Delete | Dependent::DeleteAll => "        old.delete\n".to_string(),
                    // Rails nullifies with a scoped `update_all`, leaving the
                    // loaded record as it was: a rolled-back replacement
                    // keeps its key, and so does a removed one in memory.
                    _ => {
                        let t = target.0.as_str();
                        match as_interface {
                            Some(i) => format!("        {t}.where(id: old.id).update_all({fk}: nil, {i}_type: nil)\n"),
                            None => format!("        {t}.where(id: old.id).update_all({fk}: nil)\n"),
                        }
                    }
                };
                let writer_src = format!(
                    "  def {n}=(records)\n    if new_record?\n      @__held_{n} = nil\n      records.each {{ |record| __push_{n}(record) }}\n      return records\n    end\n    held = __held_{n}_list\n    current = {n}\n    {owner}.transaction do\n      current.each do |old|\n        next if records.any? {{ |record| record.id == old.id }}\n{remove}      end\n      records.each do |record|\n        next if !record.new_record? && current.any? {{ |old| old.id == record.id }}\n        next if record.new_record? && held.any? {{ |h| h.equal?(record) }}\n        if __push_{n}(record).nil?\n          raise ActiveRecord::RecordNotSaved, \"Failed to replace {n} because one or more of the new records could not be saved.\"\n        end\n      end\n    end\n    kept = held.select {{ |h| h.new_record? && records.any? {{ |record| record.equal?(h) }} }}\n    @__held_{n} = kept.empty? ? nil : kept\n    @{n}_cache = records.dup\n    @{n}_loaded = true\n    records\n  end\nend\n"
                );
                format!("{}\n{writer_src}", src.strip_suffix("end\n").unwrap_or(&src))
            } else {
                src
            };
            if let Some(mut items) = super::validates_with::ingest_source(&src, &model.name) {
                // The association's own writer, not an app def overriding
                // it (`diagnostics::overridden_synthesized_accessors`).
                for item in &mut items {
                    if let ModelBodyItem::Method { method, .. } = item
                        && method.name.as_str() == writer
                    {
                        method.name_span = crate::span::Span::synthetic();
                    }
                }
                inserts.push((index + 1, items));
                signatures.push((model.name.clone(), n.to_string(), target.clone()));
            }
        }
        for (at, items) in inserts.into_iter().rev() {
            for (k, item) in items.into_iter().enumerate() {
                model.body.insert(at + k, item);
            }
        }
    }
    for (model, name, target) in signatures {
        let relation = crate::ty::Ty::Relation { of: target.clone() };
        let record = crate::ty::Ty::Class { id: target, args: vec![].into() };
        let takes = |ret: crate::ty::Ty| crate::ty::Ty::Fn(Box::new(crate::ty::FnTy {
            params: vec![crate::ty::Param {
                name: crate::ident::Symbol::from("record"),
                ty: record.clone().into(),
                kind: crate::ty::ParamKind::Required,
            }]
            .into(),
            block: None,
            ret: ret.into(),
            effects: Default::default(),
        }));
        let table = app.rbs_signatures.entry(model).or_default();
        table.insert(crate::ident::Symbol::from(format!("__hold_{name}")), takes(record.clone()));
        // `<<` answers the association; the record is what callers chain
        // on least, so the declared answer is the record's class only
        // where nothing better is known: none is, before analysis. Nil
        // when a persisted owner's save of it fails, as Rails' `<<` is.
        let pushed = crate::ty::Ty::Union { variants: vec![record.clone(), crate::ty::Ty::Nil].into() };
        table.insert(crate::ident::Symbol::from(format!("__push_{name}")), takes(pushed.clone()));
        // `<<` with an Array adds each element (`concat(records.flatten)`).
        table.insert(
            crate::ident::Symbol::from(format!("__push_all_{name}")),
            crate::ty::Ty::Fn(Box::new(crate::ty::FnTy {
                params: vec![crate::ty::Param {
                    name: crate::ident::Symbol::from("records"),
                    ty: crate::ty::Ty::Array { elem: record.clone().into() }.into(),
                    kind: crate::ty::ParamKind::Required,
                }]
                .into(),
                block: None,
                ret: pushed.into(),
                effects: Default::default(),
            })),
        );
        // The collection terminals that answer the held records too
        // (`lower::scope_chain` hands them the seeded relation).
        let on_relation = |ret: crate::ty::Ty| crate::ty::Ty::Fn(Box::new(crate::ty::FnTy {
            params: vec![crate::ty::Param {
                name: crate::ident::Symbol::from("rel"),
                ty: relation.clone().into(),
                kind: crate::ty::ParamKind::Required,
            }]
            .into(),
            block: None,
            ret: ret.into(),
            effects: Default::default(),
        }));
        let maybe = crate::ty::Ty::Union { variants: vec![record.clone(), crate::ty::Ty::Nil].into() };
        for (terminal, ret) in [
            ("first", maybe.clone()),
            ("last", maybe),
            ("size", crate::ty::Ty::Int),
            ("length", crate::ty::Ty::Int),
            ("empty?", crate::ty::Ty::Bool),
            ("any?", crate::ty::Ty::Bool),
            ("destroy_all", crate::ty::Ty::Array { elem: record.clone().into() }),
        ] {
            table.insert(crate::ident::Symbol::from(format!("__{name}_{terminal}")), on_relation(ret));
        }
        table.insert(
            crate::ident::Symbol::from(format!("__held_{name}_list")),
            crate::ty::Ty::Fn(Box::new(crate::ty::FnTy {
                params: vec![].into(),
                block: None,
                ret: crate::ty::Ty::Array { elem: record.clone().into() }.into(),
                effects: Default::default(),
            })),
        );
    }
}
