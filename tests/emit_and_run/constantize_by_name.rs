//! `String#constantize` on the Ruby target is Active Support's: the interpreter
//! resolves the name (`Object.const_get`), so a framework constant answers as in
//! Rails and the lookup names no app file, which a closed `case` of arms would
//! have required at load.

use super::emit_and_run;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .write("app/models/cart.rb", "class Cart\n  def self.tag\n    \"Cart\"\n  end\nend\n")
        .write(
            "app/models/cart_registry.rb",
            "class CartRegistry\n  KIND = \"Cart\".constantize\n\n  def self.resolve(name)\n    name.constantize\n  end\n\n  def self.resolve_safely(name)\n    name.safe_constantize\n  end\nend\n",
        )
}

const ASSERTIONS: &str = r##"def check(label, got, want)
  raise "#{label}: got #{got.inspect}, want #{want.inspect}" unless got == want
end
check("app constant", CartRegistry.resolve("Cart"), Cart)
check("rooted app constant", CartRegistry.resolve("::Cart"), Cart)
check("class body constantize", CartRegistry::KIND, Cart)
check("framework constant", CartRegistry.resolve("ActiveRecord::RecordNotFound"), ActiveRecord::RecordNotFound)
check("builtin constant", CartRegistry.resolve("String"), String)
check("safe framework constant", CartRegistry.resolve_safely("ActiveRecord::RecordNotFound"), ActiveRecord::RecordNotFound)
check("safe unknown", CartRegistry.resolve_safely("NoSuchCart"), nil)
check("safe unknown nested", CartRegistry.resolve_safely("Cart::NoSuchPart"), nil)
begin
  CartRegistry.resolve("NoSuchCart")
  raise "constantize of an unknown name did not raise"
rescue NameError => e
  check("unknown name", e.name, :NoSuchCart)
end
puts "constantize passed"
"##;

#[test]
fn constantize_resolves_app_and_framework_constants_as_rails_does() {
    app().run_ruby(ASSERTIONS).assert_passes();
}
