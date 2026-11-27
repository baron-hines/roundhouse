//! A nested module constant read in a `case/when` clause of an instance
//! method keeps its lexical scope.
//!
//! `Catalog::Lifecycle::Entry` declares `module Type`
//! (`private_constant :Type`) and its `notify` instance method does
//! `case event when Type::NOTIFY_OWNER_ON_ENTRY_CHANGE`. The emit
//! rooted the `when` constants at `::Type`: NameError, uninitialized
//! constant Type.
//!
//! The rooting pass looks a bare head up in the namespaces the emit opens
//! around the class (`Catalog`, `Catalog::Lifecycle`), where the
//! sibling `Catalog::Type` is known and `Catalog::Type::NOTIFY_*`
//! is not: it read the head as the compact-form shadow it guards against.
//! The class's own constants are the innermost scope, in the source as in
//! the emit, and bind first.

#[path = "support/emit_and_run.rs"]
mod emit_and_run;

const SIBLING_TYPE: &str = r#"module Catalog
  module Type
    BOOLEAN = "boolean"
  end
end
"#;

const LIFECYCLE: &str = r#"module Catalog
  module Lifecycle
    class Entry
      class InvalidNotifyParameter < StandardError; end

      module Type
        NOTIFY_OWNER_ON_ENTRY_CHANGE = :notify_owner_on_entry_change
        NOTIFY_ON_ENTRY_OWNER_DELETE = :notify_on_entry_owner_delete

        class Event
          def self.label
            "event"
          end
        end
      end
      private_constant :Type

      class << self
        #: (Integer) -> void
        def notify_owner_on_entry_change(entry)
          new.notify(Type::NOTIFY_OWNER_ON_ENTRY_CHANGE, entry)
        end

        def notify_on_entry_owner_delete(owner)
          new.notify(Type::NOTIFY_ON_ENTRY_OWNER_DELETE, owner)
        end

        def event_label
          Type::Event.label
        end
      end

      #: (Symbol, untyped) -> void
      def notify(event, object)
        @was_notified = true
        case event
        when Type::NOTIFY_OWNER_ON_ENTRY_CHANGE
          "changed #{object}"
        when Type::NOTIFY_ON_ENTRY_OWNER_DELETE
          "deleted #{object}"
        else
          raise InvalidNotifyParameter, "unsupported #{event}"
        end
      end
    end
  end
end
"#;

#[test]
fn a_nested_module_constant_in_a_when_clause_stays_scoped() {
    let run = emit_and_run::empty_app()
        .write("app/models/catalog/type.rb", SIBLING_TYPE)
        .write("app/models/catalog/lifecycle/entry.rb", LIFECYCLE)
        .run_ruby(
            "m = Catalog::Lifecycle::Entry\n\
             puts m.notify_owner_on_entry_change(1)\n\
             puts m.notify_on_entry_owner_delete(2)\n\
             puts m.event_label\n\
             begin; m.new.notify(:x, 3); rescue m::InvalidNotifyParameter => e; puts e.message; end\n",
        );
    run.assert_passes();
    assert_eq!(run.stdout.trim(), "changed 1\ndeleted 2\nevent\nunsupported x", "{}", run.stdout);
}
