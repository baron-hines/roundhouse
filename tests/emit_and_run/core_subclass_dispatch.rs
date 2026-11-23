//! An app class under a core collection (discourse's `class Enum < Hash`) answers the collection's methods its own
//! chain does not define, as Ruby's ancestry does.

use super::emit_and_run;

#[test]
fn an_app_hash_subclass_answers_hash_methods() {
    let lib = "class Ordinal < Hash\n  def initialize(*members)\n    super({})\n    update Hash[members.zip(1..members.count)]\n  end\n\n  def valid?(member)\n    key?(member)\n  end\nend\n\nclass Fruits\n  def self.all\n    @all ||= Ordinal.new(:apple, :kiwi)\n  end\n\n  def self.rows\n    Fruits.all.map { |name, id| { id: id, name: name.to_s } }\n  end\n\n  def self.names\n    Fruits.all.keys.map(&:to_s).join(\",\")\n  end\nend\n";
    let overlay = || {
        emit_and_run::real_blog().write("app/models/ordinal.rb", lib).edit(
            "app/models/article.rb",
            "  validates :title, presence: true\n",
            "  validates :title, presence: true\n\n  def self.kinds\n    @kinds ||= Ordinal.new(:draft, :live)\n  end\n\n  def self.kind_rows\n    Article.kinds.map { |name, id| { id: id, name: name.to_s } }\n  end\n\n  def self.live_kind?\n    Article.kinds.freeze.valid?(:live) && Ordinal.name == \"Ordinal\"\n  end\n",
        )
    };
    let errors = overlay().check_errors();
    assert!(errors.is_empty(), "{errors:#?}");
    overlay()
        .run_ruby("raise Fruits.rows.inspect unless Fruits.rows == [{ id: 1, name: \"apple\" }, { id: 2, name: \"kiwi\" }]\nraise Fruits.names unless Fruits.names == \"apple,kiwi\"\nraise \"valid\" unless Fruits.all.valid?(:kiwi)\nraise Article.kind_rows.inspect unless Article.kind_rows == [{ id: 1, name: \"draft\" }, { id: 2, name: \"live\" }]\nraise \"live\" unless Article.live_kind?\nputs \"core subclass passed\"\n")
        .assert_passes();
}
