//! discourse's `JsonApiKit::Resource` concerns: an `included` block that
//! declares a class-side-only `class_attribute` and then makes its reader
//! and writer private with `private_class_method`. The class methods the
//! concern adds read and write it, a subclass's body calls them, and a
//! call of the reader from outside is NoMethodError, as Rails 8.1 answers.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/concerns/declared_fields.rb", r##"module DeclaredFields
  extend ActiveSupport::Concern

  included do
    class_attribute :declared_names,
                    default: [].freeze,
                    instance_accessor: false,
                    instance_predicate: false
    private_class_method :declared_names, :declared_names=
  end

  class_methods do
    def field(name)
      self.declared_names = declared_names + [name]
    end

    def field_names = declared_names
  end
end
"##)
        .write("app/models/field_resource.rb", "class FieldResource\n  include DeclaredFields\nend\n")
        .write("app/models/topic_resource.rb", "class TopicResource < FieldResource\n  field :title\n  field :body\nend\n")
}

const ASSERTIONS: &str = r##"got = [TopicResource.field_names, FieldResource.field_names]
raise got.inspect unless got == [[:title, :body], []]
begin
  TopicResource.declared_names
  raise "declared_names is public"
rescue NoMethodError
end
puts "class attribute private passed"
"##;

#[test]
fn a_class_side_attribute_made_private_in_an_included_block_runs() {
    let errors = app().check_errors();
    assert!(errors.is_empty(), "{errors:#?}");
    app().run_ruby(ASSERTIONS).assert_passes();
}

/// An attribute with an instance accessor is not one the visibility walk
/// attributes: beside a marker, the file stays refused.
#[test]
fn a_class_attribute_with_an_instance_accessor_beside_a_marker_is_refused() {
    let refused = std::panic::catch_unwind(|| {
        emit_and_run::empty_app()
            .write("app/models/concerns/declared_fields.rb", "module DeclaredFields\n  extend ActiveSupport::Concern\n\n  included do\n    class_attribute :declared_names, default: []\n    private_class_method :declared_names\n  end\nend\n")
            .write("app/models/field_resource.rb", "class FieldResource\n  include DeclaredFields\nend\n")
            .check_errors()
    })
    .expect_err("ingest refuses the file");
    let message = refused.downcast_ref::<String>().cloned().unwrap_or_default();
    assert!(message.contains("conditional or dynamic visibility declarations are not modeled"), "{message}");
}

