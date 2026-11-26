//! `Model.reflect_on_association(name)` answers the association's macro, foreign key and `polymorphic:` as Rails'
//! AssociationReflection does (orm_adapter's `conditions_to_fields` asks `belongs_to?`, `foreign_key` and
//! `options[:polymorphic]`), and `base_class` is the class itself for a model no STI parent has.

use super::emit_and_run;

const SCRIPT: &str = r#"belongs = Comment.reflect_on_association(:article)
raise "belongs_to macro #{belongs.macro.inspect}" unless belongs.macro == :belongs_to && belongs.belongs_to?
raise "belongs_to key #{belongs.foreign_key.inspect}" unless belongs.foreign_key == "article_id"
raise "belongs_to options #{belongs.options.inspect}" unless belongs.options == {}
many = Article.reflect_on_association(:comments)
raise "has_many macro #{many.macro.inspect}" unless many.macro == :has_many && !many.belongs_to?
raise "has_many key #{many.foreign_key.inspect}" unless many.foreign_key == "article_id"
raise "none" unless Article.reflect_on_association(:nothing).nil?
raise "base_class" unless Article.base_class == Article
puts "association reflection passed"
"#;

#[test]
fn an_association_reflection_answers_its_macro_and_foreign_key() {
    let run = emit_and_run::real_blog().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("association reflection passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn an_association_reflection_answers_its_macro_and_foreign_key_natively() {
    let run = emit_and_run::real_blog().run_spinel(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("association reflection passed"), "{}", run.stdout);
}

/// `base_class` under STI: a subclass whose namespace holds the base's name still answers the top-level base, and a
/// subclass of a class that answers `base_class` itself inherits that answer.
fn sti_app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", "class ApplicationRecord < ActiveRecord::Base\n  self.abstract_class = true\nend\n")
        .write("app/controllers/application_controller.rb", "class ApplicationController < ActionController::Base\nend\n")
        .write("db/schema.rb", "ActiveRecord::Schema.define do\n  create_table \"categories\", force: :cascade do |t|\n    t.string \"name\", null: false\n    t.string \"type\"\n  end\nend\n")
        .write("app/models/category.rb", "class Category < ApplicationRecord\nend\n")
        .write("app/models/admin/category.rb", "module Admin\n  class Category < ::Category\n  end\nend\n")
        .write("app/models/special_category.rb", "class SpecialCategory < Category\n  def self.base_class\n    SpecialCategory\n  end\nend\n")
        .write("app/models/deep_category.rb", "class DeepCategory < SpecialCategory\nend\n")
        .write("config/routes.rb", "Rails.application.routes.draw do\nend\n")
}

const STI_SCRIPT: &str = r#"raise "plain #{Category.base_class}" unless Category.base_class == Category
raise "namespaced #{Admin::Category.base_class}" unless Admin::Category.base_class == ::Category
raise "inherited override #{DeepCategory.base_class}" unless DeepCategory.base_class == SpecialCategory
puts "sti base_class passed"
"#;

#[test]
fn base_class_answers_the_top_level_base_and_an_inherited_override() {
    let run = sti_app().run_ruby(STI_SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("sti base_class passed"), "{}", run.stdout);
}
