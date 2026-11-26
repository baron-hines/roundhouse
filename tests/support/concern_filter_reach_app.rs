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

/// A controller with its own filters on either side of `include`.
fn traced_controller(name: &str, includes: &str) -> String {
    format!(
        "class {name} < ApplicationController\n  before_action :first\n  include {includes}\n  before_action :last\n\n  def index\n    render plain: \"trace=#{{@trace}}\"\n  end\n\n{FIRST}{}end\n",
        tracer(&["last"], &[])
    )
}

/// A plain module whose `self.included` hook is `hook`.
fn hooked(name: &str, hook: &str, steps: &[&str]) -> String {
    format!(
        "module {name}\n  def self.included(base)\n    {hook}\n  end\n\n{}end\n",
        tracer(steps, &[])
    )
}

/// [`hooked`], with `extend ActiveSupport::Concern`.
fn hooked_concern(name: &str, hook: &str, steps: &[&str]) -> String {
    hooked(name, hook, steps).replacen("\n", "\n  extend ActiveSupport::Concern\n", 1)
}

const GUARDED_HOOK: &str = "base.before_action :hook_step if base.respond_to?(:before_action)";
const METHODS_HOOK: &str = "base.instance_eval do\n      before_action(:eval_step) if base.methods.include?(:before_action)\n    end";
const SELF_GUARD_HOOK: &str = "base.class_eval do\n      before_action :eval_step if respond_to?(:before_action)\n    end";

/// Every hook shape the compiler models, and the Rails rules for who
/// receives the filter.
fn with_hooks(app: Overlay) -> Overlay {
    let mut app = app
        .write("app/controllers/concerns/direct_hook.rb", &hooked("DirectHook", "base.before_action :hook_step", &["hook_step"]))
        .write("app/controllers/concerns/guarded_hook.rb", &hooked("GuardedHook", GUARDED_HOOK, &["hook_step"]))
        .write("app/controllers/concerns/concern_hook.rb", &hooked_concern("ConcernHook", GUARDED_HOOK, &["hook_step"]))
        .write("app/controllers/concerns/methods_hook.rb", &hooked_concern("MethodsHook", METHODS_HOOK, &["eval_step"]))
        .write("app/controllers/concerns/self_guard_hook.rb", &hooked("SelfGuardHook", SELF_GUARD_HOOK, &["eval_step"]))
        .write(
            "app/controllers/concerns/block_and_hook.rb",
            &concern(
                "BlockAndHook",
                "  included do\n    before_action :block_step\n  end\n\n  def self.included(base)\n    super\n    base.before_action :hook_step\n  end\n",
                &tracer(&["block_step", "hook_step"], &[]),
            ),
        )
        .write(
            "app/controllers/concerns/hook_dependency.rb",
            &concern("HookDependency", "  included do\n    before_action :dep_step\n  end\n", &tracer(&["dep_step"], &[])),
        )
        .write(
            "app/controllers/concerns/ordered_hook.rb",
            &concern(
                "OrderedHook",
                "  include HookDependency\n\n  included do\n    before_action :block_step\n  end\n\n  def self.included(base)\n    super\n    base.before_action :hook_step\n  end\n",
                &tracer(&["block_step", "hook_step"], &[]),
            ),
        )
        // A concern's hook passes through concerns: `append_features`
        // includes it again on the controller.
        .write("app/controllers/concerns/through_concern.rb", &concern("ThroughConcern", "  include ConcernHook\n", ""))
        .write("app/controllers/concerns/through_two.rb", &concern("ThroughTwo", "  include ThroughConcern\n", ""))
        // A plain module's hook does not: it ran once, on the concern.
        .write(
            "app/controllers/concerns/over_plain_hook.rb",
            &concern("OverPlainHook", "  include GuardedHook\n", "  def over_label\n    \"over\"\n  end\n"),
        )
        // `base` is whoever included it: here the module, where the guard is false.
        .write(
            "app/controllers/concerns/plain_over_hook.rb",
            "module PlainOverHook\n  include ConcernHook\n\n  def plain_label\n    \"plain\"\n  end\nend\n",
        );
    for (file, class, includes) in [
        ("direct_hook", "DirectHookController", "DirectHook"),
        ("guarded_hook", "GuardedHookController", "GuardedHook"),
        ("methods_hook", "MethodsHookController", "MethodsHook"),
        ("self_guard_hook", "SelfGuardHookController", "SelfGuardHook"),
        ("block_and_hook", "BlockAndHookController", "BlockAndHook"),
        ("ordered_hook", "OrderedHookController", "OrderedHook"),
        ("through_concern", "ThroughConcernController", "ThroughConcern"),
        ("through_two", "ThroughTwoController", "ThroughTwo"),
        ("over_plain_hook", "OverPlainHookController", "OverPlainHook"),
        ("plain_over_hook", "PlainOverHookController", "PlainOverHook"),
    ] {
        app = app.write(&format!("app/controllers/{file}_controller.rb"), &traced_controller(class, includes));
    }
    app
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
    with_hooks(app).write(
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
        "Rails.application.routes.draw do\n  get \"/admin/locals\", to: \"admin/locals#index\"\n  get \"/signed\", to: \"signed#index\"\n  get \"/guarded\", to: \"guarded#index\"\n  get \"/two_levels\", to: \"two_levels#index\"\n  get \"/scoped\", to: \"scoped#index\"\n  get \"/stacked\", to: \"stacked#index\"\n  get \"/mixed\", to: \"mixed#index\"\n  get \"/ordered\", to: \"ordered#index\"\n  get \"/skipping\", to: \"skipping#index\"\n  get \"/direct_hook\", to: \"direct_hook#index\"\n  get \"/guarded_hook\", to: \"guarded_hook#index\"\n  get \"/methods_hook\", to: \"methods_hook#index\"\n  get \"/self_guard_hook\", to: \"self_guard_hook#index\"\n  get \"/block_and_hook\", to: \"block_and_hook#index\"\n  get \"/ordered_hook\", to: \"ordered_hook#index\"\n  get \"/through_concern\", to: \"through_concern#index\"\n  get \"/through_two\", to: \"through_two#index\"\n  get \"/over_plain_hook\", to: \"over_plain_hook#index\"\n  get \"/plain_over_hook\", to: \"plain_over_hook#index\"\n  get \"/ordered/show\", to: \"ordered#show\"\n  get \"/skipping/show\", to: \"skipping#show\"\nend\n",
    )
}

/// [`app`] with a plain class including a hooked concern: `base` is not
/// a controller. The strict targets carry no plain Ruby class.
pub fn app_with_plain_includer() -> Overlay {
    app().write(
        "app/models/hooked_report.rb",
        "class HookedReport\n  include MethodsHook\n\n  def label\n    \"report\"\n  end\nend\n",
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

def trace(controller)
  run_action(controller, :index, {}).body
end

expect("direct hook", trace(DirectHookController.new), "trace=first hook_step last ")
expect("guarded hook", trace(GuardedHookController.new), "trace=first hook_step last ")
expect("eval hook, base.methods guard", trace(MethodsHookController.new), "trace=first eval_step last ")
expect("eval hook, respond_to? guard", trace(SelfGuardHookController.new), "trace=first eval_step last ")
expect("block then hook", trace(BlockAndHookController.new), "trace=first block_step hook_step last ")
expect("dependency, block, hook", trace(OrderedHookController.new), "trace=first dep_step block_step hook_step last ")
expect("concern hook through a concern", trace(ThroughConcernController.new), "trace=first hook_step last ")
expect("concern hook through two concerns", trace(ThroughTwoController.new), "trace=first hook_step last ")
expect("plain hook under a concern", trace(OverPlainHookController.new), "trace=first last ")
expect("concern hook under a plain module", trace(PlainOverHookController.new), "trace=first last ")
expect("hook on a plain class", HookedReport.new.label, "report")

raise $mismatches.join("; ") unless $mismatches.empty?
puts "concern filters reached"
"##;

/// CRuby autoloads nothing here: the Spinel build compiles every
/// controller in, a `ruby -e` script has to load them.
pub const LOAD_CONTROLLERS: &str =
    "require File.expand_path(\"app/models/hooked_report.rb\")\nDir[File.expand_path(\"app/controllers/**/*_controller.rb\")].sort.each { |file| require file }\n";
