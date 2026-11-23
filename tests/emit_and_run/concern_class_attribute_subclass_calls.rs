//! A plain class includes a Concern whose `included` block declares a
//! `class_attribute` and whose `class_methods` write it; a subclass calls
//! those class methods in its body. Ruby runs the calls as the subclass
//! loads, so the subclass's reads see what they wrote and the parent keeps
//! its default, as Rails 8.1 answers. The calls were dropped from the
//! emitted body, and the reads answered the default.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/concerns/declared_fields.rb", r##"module DeclaredFields
  extend ActiveSupport::Concern

  included do
    class_attribute :declared_names, default: [].freeze, instance_accessor: false, instance_predicate: false
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
        .write("app/models/topic_resource.rb", "class TopicResource < FieldResource\n  field :title\n  field :body\n  SNAPSHOT = field_names\nend\n")
}

#[test]
fn a_subclass_body_calling_its_parent_s_class_methods_runs_them() {
    let errors = app().check_errors();
    assert!(errors.is_empty(), "{errors:#?}");
    app()
        .run_ruby("got = [TopicResource.field_names, FieldResource.field_names, TopicResource::SNAPSHOT]\nraise got.inspect unless got == [[:title, :body], [], [:title, :body]]\nputs \"subclass calls passed\"\n")
        .assert_passes();
}
