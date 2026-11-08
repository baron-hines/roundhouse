//! The app `concern_filter_reach` and `concern_filter_reach_spinel` run.
//! Every status and trace asserted on it is what Rails 8.1.4 answers for
//! the same concerns and controllers.

use super::emit_and_run::{empty_app, Overlay};

const CHECK: &str = "  def check_signature\n    head :unauthorized if params[\"sig\"].to_s.empty?\n  end\n";

const FIRST: &str = "  def first\n    @after_trace = \"\"\n    @trace = \"first \"\n  end\n";

fn concern(name: &str, included: &str, body: &str) -> String {
    format!(
        "module {name}\n  extend ActiveSupport::Concern\n{included}\n{body}end\n"
    )
}

fn controller(name: &str, includes: &str) -> String {
    format!(
        "class {name} < ApplicationController\n  include {includes}\n\n  def index\n    render plain: \"ok\"\n  end\nend\n"
    )
}

/// A filter that records itself. `head` ends the chain, so `first` etc.
/// only show up when the request went through.
fn tracer(names: &[&str], after: &[&str]) -> String {
    let mut out = String::new();
    for name in names {
        out.push_str(&format!("  def {name}\n    @trace = \"#{{@trace}}{name} \"\n  end\n"));
    }
    for name in after {
        out.push_str(&format!("  def {name}\n    @after_trace = \"#{{@after_trace}}{name} \"\n  end\n"));
    }
    out
}

pub fn app() -> Overlay {
    let mut app = empty_app()
        .write(
            "app/controllers/application_controller.rb",
            "class ApplicationController < ActionController::Base\nend\n",
        )
        .write(
            "app/controllers/concerns/signed.rb",
            &concern("Signed", "  included do\n    before_action :check_signature\n  end\n", CHECK),
        )
        .write(
            "app/controllers/concerns/guarded_signed.rb",
            &concern(
                "GuardedSigned",
                "  included do\n    before_action :check_signature, if: :signature_on? if respond_to?(:before_action)\n  end\n",
                &format!("{CHECK}  def signature_on?\n    true\n  end\n"),
            ),
        )
        // Concerns whose whole body is `extend` and `include`.
        .write("app/controllers/concerns/only_signed.rb", &concern("OnlySigned", "  include Signed\n", ""))
        .write(
            "app/controllers/concerns/only_guarded.rb",
            &concern("OnlyGuarded", "  include GuardedSigned\n", ""),
        )
        .write("app/controllers/concerns/two_levels.rb", &concern("TwoLevels", "  include OnlyGuarded\n", ""))
        // A plain module: `GuardedSigned`'s block runs on the module, where
        // `respond_to?(:before_action)` is false, so no filter registers.
        .write(
            "app/controllers/concerns/plain_mixin.rb",
            "module PlainMixin\n  include GuardedSigned\n\n  def mixin_label\n    \"mixin\"\n  end\nend\n",
        )
        // Written relative to the enclosing namespace.
        .write(
            "app/controllers/concerns/scoped.rb",
            "module Scoped\n  module Inner\n    extend ActiveSupport::Concern\n\n    included do\n      before_action :check_signature\n    end\n\n    def check_signature\n      head :unauthorized if params[\"sig\"].to_s.empty?\n    end\n  end\n\n  module Middle\n    extend ActiveSupport::Concern\n    include Inner\n  end\nend\n",
        )
        // A controller naming a concern relative to its own namespace.
        .write(
            "app/controllers/concerns/admin/local_signed.rb",
            "module Admin\n  module LocalSigned\n    extend ActiveSupport::Concern\n\n    included do\n      before_action :check_signature\n    end\n\n    def check_signature\n      head :unauthorized if params[\"sig\"].to_s.empty?\n    end\n  end\nend\n",
        )
        .write(
            "app/controllers/admin/locals_controller.rb",
            "module Admin\n  class LocalsController < ApplicationController\n    include LocalSigned\n\n    def index\n      render plain: \"ok\"\n    end\n  end\nend\n",
        )
        // A concern under lib/, reached through another one.
        .write(
            "lib/gates/lib_signed.rb",
            "module Gates\n  module LibSigned\n    extend ActiveSupport::Concern\n\n    included do\n      before_action :check_signature, if: :signature_on? if respond_to?(:before_action)\n    end\n\n    def check_signature\n      head :unauthorized if params[\"sig\"].to_s.empty?\n    end\n\n    def signature_on?\n      true\n    end\n  end\nend\n",
        )
        .write(
            "lib/gates/stack.rb",
            "module Gates\n  module Stack\n    extend ActiveSupport::Concern\n    include Gates::LibSigned\n\n    def stack_label\n      \"stack\"\n    end\n  end\nend\n",
        )
        // Registration order, `only:`, `after_action` and skip, two levels deep.
        .write(
            "app/controllers/concerns/order_inner.rb",
            &concern(
                "OrderInner",
                "  included do\n    before_action :inner\n    after_action :inner_after\n  end\n",
                &tracer(&["inner"], &["inner_after"]),
            ),
        )
        .write(
            "app/controllers/concerns/order_middle.rb",
            &concern(
                "OrderMiddle",
                "  include OrderInner\n\n  included do\n    before_action :middle, only: :index\n    after_action :middle_after\n  end\n",
                &tracer(&["middle"], &["middle_after"]),
            ),
        )
        .write("app/controllers/concerns/order_outer.rb", &concern("OrderOuter", "  include OrderMiddle\n", ""))
        .write(
            "app/controllers/concerns/order_skip.rb",
            &concern("OrderSkip", "  include OrderOuter\n\n  included do\n    skip_before_action :inner, only: :show\n  end\n", ""),
        );
    for (file, class, includes) in [
        ("signed", "SignedController", "OnlySigned"),
        ("guarded", "GuardedController", "OnlyGuarded"),
        ("two_levels", "TwoLevelsController", "TwoLevels"),
        ("scoped", "ScopedController", "Scoped::Middle"),
        ("stacked", "StackedController", "Gates::Stack"),
    ] {
        app = app.write(&format!("app/controllers/{file}_controller.rb"), &controller(class, includes));
    }
    let actions = "  def index\n    render plain: \"trace=#{@trace}\"\n  end\n\n  def show\n    render plain: \"trace=#{@trace}\"\n  end\n\n  def after_trace\n    @after_trace\n  end\n\n";
    app.write(
        "app/controllers/mixed_controller.rb",
        "class MixedController < ApplicationController\n  include PlainMixin\n\n  def index\n    render plain: \"label=#{mixin_label}\"\n  end\nend\n",
    )
    .write(
        "app/controllers/ordered_controller.rb",
        &format!(
            "class OrderedController < ApplicationController\n  before_action :first\n  include OrderOuter\n  before_action :last\n\n{actions}{FIRST}{}end\n",
            tracer(&["last"], &[])
        ),
    )
    .write(
        "app/controllers/skipping_controller.rb",
        &format!(
            "class SkippingController < ApplicationController\n  before_action :first\n  include OrderSkip\n  before_action :last\n\n{actions}{FIRST}{}end\n",
            tracer(&["last"], &[])
        ),
    )
    .write(
        "config/routes.rb",
        "Rails.application.routes.draw do\n  get \"/admin/locals\", to: \"admin/locals#index\"\n  get \"/signed\", to: \"signed#index\"\n  get \"/guarded\", to: \"guarded#index\"\n  get \"/two_levels\", to: \"two_levels#index\"\n  get \"/scoped\", to: \"scoped#index\"\n  get \"/stacked\", to: \"stacked#index\"\n  get \"/mixed\", to: \"mixed#index\"\n  get \"/ordered\", to: \"ordered#index\"\n  get \"/skipping\", to: \"skipping#index\"\n  get \"/ordered/show\", to: \"ordered#show\"\n  get \"/skipping/show\", to: \"skipping#show\"\nend\n",
    )
}

pub const ASSERTIONS: &str = r##"
$mismatches = []

def expect(label, actual, wanted)
  $mismatches << "#{label}: got #{actual.inspect}, want #{wanted.inspect}" unless actual == wanted
end

def run_action(controller, action, params)
  controller.params = params
  controller.process_action(action)
  controller
end

def gated(name, unsigned, signed)
  expect("#{name} unsigned", run_action(unsigned, :index, {}).status, 401)
  run_action(signed, :index, { "sig" => "s" })
  expect("#{name} signed", signed.status, 200)
  expect("#{name} body", signed.body, "ok")
end

gated("only-includes concern", SignedController.new, SignedController.new)
gated("only-includes concern, guarded filter", GuardedController.new, GuardedController.new)
gated("two only-includes concerns", TwoLevelsController.new, TwoLevelsController.new)
gated("relative include", ScopedController.new, ScopedController.new)
gated("lib concern through a concern", StackedController.new, StackedController.new)
gated("relative include in a controller", Admin::LocalsController.new, Admin::LocalsController.new)

mixed = run_action(MixedController.new, :index, {})
expect("plain module status", mixed.status, 200)
expect("plain module body", mixed.body, "label=mixin")

ordered = run_action(OrderedController.new, :index, {})
expect("ordered index", ordered.body, "trace=first inner middle last ")
expect("ordered after_action", ordered.after_trace, "middle_after inner_after ")
expect("ordered show", run_action(OrderedController.new, :show, {}).body, "trace=first inner last ")

expect("skip index", run_action(SkippingController.new, :index, {}).body, "trace=first inner middle last ")
expect("skip show", run_action(SkippingController.new, :show, {}).body, "trace=first last ")

raise $mismatches.join("; ") unless $mismatches.empty?
puts "concern filters reached"
"##;

/// CRuby autoloads nothing here: the Spinel build compiles every
/// controller in, a `ruby -e` script has to load them.
pub const LOAD_CONTROLLERS: &str =
    "Dir[File.expand_path(\"app/controllers/**/*_controller.rb\")].sort.each { |file| require file }\n";
