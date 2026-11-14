//! view_component compiled from its installed source: a view's `render(GreetingComponent.new(...))` reaches the gem's
//! `render_in`, and the component's sidecar template, compiled into its `call` when the app is read, writes the markup
//! view_component 3.23.2 renders for it on Rails 7.2.2.1 (maybe's versions): `render?`, a component method, an ivar and
//! a view helper; and with a block, its `content` and its `renders_one` / `renders_many` slots (a lambda's among
//! them, whose `html_safe` markup is not escaped again, and a block's markup, which Rails captures safe), escaped as
//! Rails does.

#[path = "prerequisite.rs"]
mod prerequisite;
use super::emit_and_run;

const GEMS: [(&str, &str); 2] = [("view_component", "3.23.2"), ("method_source", "1.1.0")];

fn installed(name: &str, version: &str) -> Option<std::path::PathBuf> {
    let out = std::process::Command::new("ruby")
        .args(["-e", &format!("print Gem::Specification.find_by_name('{name}', '{version}').gem_dir")])
        .output();
    match out {
        Ok(out) if out.status.success() => Some(std::path::PathBuf::from(String::from_utf8_lossy(&out.stdout).into_owned())),
        _ => {
            prerequisite::missing(&format!("ruby with {name} {version} installed"));
            None
        }
    }
}

fn files(root: &std::path::Path, dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            files(root, &path, out);
        } else {
            out.push(path.strip_prefix(root).unwrap().to_path_buf());
        }
    }
}

const COMPONENT: &str = "class GreetingComponent < ViewComponent::Base\n  def initialize(name:, count: 0)\n    @name = name\n    @count = count\n  end\n\n  def shout = @name.upcase\n\n  def render? = @count >= 0\nend\n";

const TEMPLATE: &str = "<p class=\"greeting\">Hello, <%= shout %>! (<%= pluralize(@count, \"article\") %>)</p>\n";

const CARD: &str = "class CardComponent < ViewComponent::Base\n  renders_one :header\n  renders_many :items\n  renders_one :footer, ->(note:, &block) { \"<small>#{note}</small>\".html_safe }\n\n  def initialize(tone: \"plain\")\n    @tone = tone\n  end\nend\n";

const CARD_TEMPLATE: &str = "<div class=\"card card-<%= @tone %>\">\n  <% if header? %><h2><%= header %></h2><% end %>\n  <ul><% items.each do |item| %><li><%= item %></li><% end %></ul>\n  <div class=\"body\"><%= content %></div>\n  <%= footer %>\n</div>\n";

#[allow(dead_code)]
pub const SCRIPT: &str = r#"io = +""
Views::Articles.index_into(io, Article.all, nil, nil)
expected = "<p class=\"greeting\">Hello, BLOG! (0 articles)</p>"
raise "component:\n#{io}" unless io.include?(expected)
card = "<div class=\"card card-warm\">\n  <h2>Title &lt;b&gt;</h2>\n  <ul><li>one</li><li>two</li><li><b>three</b></li></ul>\n  <div class=\"body\">\n  \n  Body text\n</div>\n  <small>fine <print></small>\n</div>"
raise "card:\n#{io}" unless io.include?(card)
puts "view_component passed"
"#;

/// The components on the blog, rendered straight from the script rather than through a view (the view's render runs
/// the whole page's queries and partials natively too): the card's block is Ruby's, which the gem captures as Rails does.
/// Not an app of nothing else: a Spinel tree with no model leaves the runtime record's schema untyped.
#[allow(dead_code)]
pub fn native_app() -> Option<emit_and_run::Overlay> {
    let mut overlay = with_ui(emit_and_run::real_blog())
        .write("app/components/greeting_component.rb", COMPONENT)
        .write("app/components/greeting_component.html.erb", TEMPLATE)
        .write("app/components/card_component.rb", CARD)
        .write("app/components/card_component.html.erb", CARD_TEMPLATE)
        .write(
            "app/lib/component_report.rb",
            "class ComponentReport\n  def self.lines\n    view = ActionView::Base.new\n    greeting = GreetingComponent.new(name: \"blog\", count: 0).render_in(view)\n    card = CardComponent.new(tone: \"warm\").render_in(view) do |c|\n      c.with_header { \"Title <b>\" }\n      c.with_item { \"one\" }\n      c.with_item { \"two\" }\n      c.with_footer(note: \"fine <print>\")\n      \"\\n  Body text\\n\"\n    end\n    [greeting.to_s, card.to_s]\n  end\nend\n",
        );
    for (name, version) in GEMS {
        overlay = with_gem(overlay, name, version)?;
    }
    Some(overlay)
}

/// What view_component 3.23.2 renders for `ComponentReport.lines` on Rails 7.2.2.1 (the gem run on a stub of Rails).
#[allow(dead_code)]
pub const NATIVE_SCRIPT: &str = "ComponentReport.lines.each { |line| puts line }\nUiReport.lines.each { |line| puts line }\n";
#[allow(dead_code)]
pub const NATIVE_EXPECTED: &str = "<p class=\"greeting\">Hello, BLOG! (0 articles)</p>\n<div class=\"card card-warm\">\n  <h2>Title &lt;b&gt;</h2>\n  <ul><li>one</li><li>two</li></ul>\n  <div class=\"body\">\n  Body text\n</div>\n  <small>fine <print></small>\n</div>\n";

fn gemfiles(fixture: &std::path::Path) -> (String, String) {
    let lock = std::fs::read_to_string(fixture.join("Gemfile.lock")).unwrap();
    let lock = lock
        // Not a bare "    rack (": a dependency line ("      rack (>= 2.2.4)") holds it too.
        .replacen(
            "\n    rack (",
            "\n    method_source (1.1.0)\n    view_component (3.23.2)\n      activesupport (>= 5.2.0, < 8.1)\n      concurrent-ruby (~> 1)\n      method_source (~> 1.0)\n    rack (",
            1,
        )
        .replacen("DEPENDENCIES\n", "DEPENDENCIES\n  view_component\n", 1);
    let gemfile = std::fs::read_to_string(fixture.join("Gemfile")).unwrap() + "gem \"view_component\"\n";
    (gemfile, lock)
}

fn with_gem(mut overlay: emit_and_run::Overlay, name: &str, version: &str) -> Option<emit_and_run::Overlay> {
    let gem = installed(name, version)?;
    let mut list = Vec::new();
    files(&gem, &gem.join("lib"), &mut list);
    files(&gem, &gem.join("app"), &mut list);
    for file in list {
        let Ok(text) = std::fs::read_to_string(gem.join(&file)) else { continue };
        overlay = overlay.write(&format!("vendor/bundle/ruby/3.4.0/gems/{name}-{version}/{}", file.display()), &text);
    }
    Some(overlay)
}

#[allow(dead_code)]
pub fn app() -> Option<emit_and_run::Overlay> {
    let fixture = roundhouse::fixtures::real_blog();
    let (gemfile, lock) = gemfiles(fixture);
    let mut overlay = emit_and_run::real_blog()
        .write("Gemfile", &gemfile)
        .write("Gemfile.lock", &lock)
        .write(".bundle/config", "---\nBUNDLE_PATH: \"vendor/bundle\"\n")
        .write("app/components/greeting_component.rb", COMPONENT)
        .write("app/components/greeting_component.html.erb", TEMPLATE)
        .write("app/components/card_component.rb", CARD)
        .write("app/components/card_component.html.erb", CARD_TEMPLATE)
        .edit(
            "app/views/articles/index.html.erb",
            "<% content_for :title, \"Articles\" %>\n",
            "<% content_for :title, \"Articles\" %>\n<%= render(GreetingComponent.new(name: \"blog\", count: @articles.size)) %>\n<%= render(CardComponent.new(tone: \"warm\")) do |card| %>\n  <% card.with_header { \"Title <b>\" } %>\n  <% card.with_item { \"one\" } %>\n  <% card.with_item { \"two\" } %>\n  <% card.with_item do %><b>three</b><% end %>\n  <% card.with_footer(note: \"fine <print>\") %>\n  Body text\n<% end %>\n",
        );
    for (name, version) in GEMS {
        overlay = with_gem(overlay, name, version)?;
    }
    Some(overlay)
}

/// What the gem still does that no tree answers, none of which these renders reach: its previews (a development-only
/// page of the engine) and test helpers, the engine's Railtie and `Rails::Engine`, the runtime template compiler this
/// tree replaces (`ActionView::Template`, its `class_eval`s), translations (`I18n` internals, keyword splats through
/// `super`), collections (`ActionView::PartialIteration`), and method_source's computed requires.
#[allow(dead_code)]
pub const KNOWN_GAPS: [&str; 11] = [
    "@render_args has no known type",
    "@example_name has no known type",
    "@preview has no known type",
    "@view_renderer has no known type",
    "render of an object not supported (all targets)",
    "conditional or dynamic visibility declarations are not modeled",
    "railtie middleware not supported (all targets)",
    "constant not supported (all targets): Rails::Engine",
    "constant not supported (all targets): I18n::RESERVED_KEYS",
    "constant not supported (all targets): ActionView::PartialIteration",
    "file_statement_unsupported not supported (ruby)",
];


/// A design system's components on the blog, shaped as maybe's are: a base component whose `attr_reader :icon` shares
/// its name with an app helper, a subclass whose `initialize` passes its keywords to the base's, `content_tag(:button,
/// **opts, &block)` and `link_to href, **opts do … end` wrapping a template block, and `helpers.icon` reaching an app
/// helper that calls a helper gem's `glyph_tag`. The gem (`glyphs`, written here, shaped as lucide-rails is) mixes its
/// helper into the view from a Railtie initializer and reads its gzipped glyphs from a directory beside its `lib/`.
#[allow(dead_code)]
pub fn ui_app() -> Option<emit_and_run::Overlay> {
    let mut overlay = with_ui(emit_and_run::real_blog());
    for (name, version) in GEMS {
        overlay = with_gem(overlay, name, version)?;
    }
    Some(overlay)
}

fn with_ui(overlay: emit_and_run::Overlay) -> emit_and_run::Overlay {
    let fixture = roundhouse::fixtures::real_blog();
    let (gemfile, lock) = gemfiles(fixture);
    let lock = lock
        .replacen("\n    method_source (1.1.0)\n", "\n    glyphs (0.1.0)\n      railties (>= 4.1.0)\n    method_source (1.1.0)\n", 1)
        .replacen("DEPENDENCIES\n", "DEPENDENCIES\n  glyphs\n", 1);
    let gem = "vendor/bundle/ruby/3.4.0/gems/glyphs-0.1.0";
    overlay
        .write("Gemfile", &(gemfile + "gem \"glyphs\"\n"))
        .write("Gemfile.lock", &lock)
        .write(".bundle/config", "---\nBUNDLE_PATH: \"vendor/bundle\"\n")
        .write("roundhouse.yml", "extensions:\n  - config/roundhouse\n")
        .write("config/roundhouse/gems.yml", "source_gems:\n  - {gem: \"glyphs\"}\n")
        .write(&format!("{gem}/glyphs.gemspec"), GLYPHS_GEMSPEC)
        .write(&format!("{gem}/lib/glyphs.rb"), GLYPHS_ENTRY)
        .write(&format!("{gem}/lib/glyphs/store.rb"), GLYPHS_STORE)
        .write(&format!("{gem}/lib/glyphs/helper.rb"), GLYPHS_HELPER)
        .write(&format!("{gem}/lib/glyphs/railtie.rb"), GLYPHS_RAILTIE)
        .write_bytes(&format!("{gem}/data/glyphs.bin.gz"), &GLYPHS_DATA)
        .write("app/helpers/application_helper.rb", UI_HELPER)
        .write("app/components/ui/base_button.rb", UI_BASE)
        .write("app/components/ui/button.rb", UI_BUTTON)
        .write("app/components/ui/button.html.erb", UI_BUTTON_TEMPLATE)
        .write("app/components/ui/link.rb", "class Ui::Link < Ui::BaseButton\nend\n")
        .write("app/components/ui/link.html.erb", UI_LINK_TEMPLATE)
        .write("app/components/ui/strip.rb", UI_STRIP)
        .write("app/components/ui/sheet.rb", UI_SHEET)
        .write("app/lib/ui_report.rb", UI_REPORT)
}

/// What Rails 7.2.2.1 and view_component 3.23.2 render for `UiReport.lines` with the `glyphs` gem loaded, but
/// for the last line's form: that is the form a view's `button_to … do` renders here (`view_to_library::helpers::
/// button_to_wrapper_markup`), whose attribute order, void tags and empty token input outside a request are its own.
#[allow(dead_code)]
pub const UI_EXPECTED: &str = "<button class=\"btn text-ink\">\n  \n  <span>Save &lt;now&gt;</span>\n</button>\n<button class=\"btn bg-ink text-paper btn-icon\" data-turbo-confirm=\"Sure?\">\n  <svg aria-hidden=\"true\" viewBox=\"0 0 24 24\" fill=\"none\" class=\"shrink-0 w-4 h-4 fg-ink\"><path d=\"M5 12h14\"></path><path d=\"M12 5v14\"></path></svg>\n  <span>Add</span>\n</button>\n<a class=\"btn bg-ink text-paper btn-icon\" href=\"/home\">\n  <svg aria-hidden=\"true\" viewBox=\"0 0 24 24\" fill=\"none\" class=\"shrink-0 w-5 h-5\"><path d=\"M5 12h14\"></path></svg>\n  Home\n</a>\n<form action=\"/items/1\" method=\"post\" class=\"button_to\"><input type=\"hidden\" name=\"_method\" value=\"delete\"><button type=\"submit\" class=\"btn bg-ink text-paper\" data-turbo-confirm=\"Gone?\">\n  \n  <span>Drop</span>\n</button><input type=\"hidden\" name=\"authenticity_token\" value=\"\"></form>\n<nav class=\"strip\">\n    <button id=\"a\" type=\"button\" class=\"on\">One &lt;1&gt;</button>\n    <button id=\"b\" type=\"button\" class=\"off\">2 &gt; 1</button>\n    <a class=\"btn text-ink\" href=\"/out\">\n  \n  Out\n</a>\n</nav>\n<section class=\"sheet\" data-controller=\"sheet\" id=\"p1\">\n  <header class=\"panel-head\"><h2>Edit &lt;it&gt;</h2> a &amp; b <button class=\"btn text-ink\">\n  \n  <span>x</span>\n</button></header>\n  <span class=\"tab\">Only</span>\n</section>\n";

const GLYPHS_GEMSPEC: &str = "Gem::Specification.new do |s|\n  s.name = \"glyphs\"\n  s.version = \"0.1.0\"\n  s.summary = \"Inline SVG glyphs\"\n  s.authors = [\"roundhouse\"]\n  s.files = Dir[\"lib/**/*.rb\"] + Dir[\"data/*\"]\n  s.add_dependency \"railties\", \">= 4.1.0\"\nend\n";

const GLYPHS_ENTRY: &str = "require \"active_support/gzip\"\nrequire_relative \"glyphs/store\"\nif defined?(Rails)\n  require_relative \"glyphs/helper\"\n  require_relative \"glyphs/railtie\"\nend\n\nmodule Glyphs\n  DEFAULT_OPTIONS = { \"aria-hidden\" => \"true\", \"viewBox\" => \"0 0 24 24\", \"fill\" => \"none\" }.freeze\n\n  module_function\n\n  def default_options\n    DEFAULT_OPTIONS\n  end\n\n  ROOT = Pathname.new(File.dirname(__FILE__)).join(\"..\")\nend\n";

const GLYPHS_STORE: &str = "module Glyphs\n  module Store\n    module_function\n\n    def glyph(named)\n      named = named.to_s\n      raise ArgumentError, \"Unknown glyph #{named}\" unless all.key?(named)\n\n      all[named]\n    end\n\n    def all\n      return @all if @all\n\n      data = ActiveSupport::Gzip.decompress(File.read(Glyphs::ROOT.join(\"data/glyphs.bin.gz\")))\n      @all = data.split(\"\\x00\").to_h { |x| x.split(\",\", 2) }\n    end\n  end\nend\n";

const GLYPHS_HELPER: &str = "module Glyphs\n  module Helper\n    def glyph_tag(named, **options)\n      content_tag(:svg, Store.glyph(named).html_safe, Glyphs.default_options.merge(**options))\n    end\n  end\nend\n";

const GLYPHS_RAILTIE: &str = "module Glyphs\n  class Railtie < Rails::Railtie\n    initializer \"glyphs.helper\" do\n      ActionView::Base.include Glyphs::Helper\n    end\n  end\nend\n";

/// `gzip -9 -n` of `plus,<path d="M5 12h14"></path><path d="M12 5v14"></path>\0minus,<path d="M5 12h14"></path>`.
const GLYPHS_DATA: [u8; 64] = *include_bytes!("glyphs.bin.gz");

const UI_HELPER: &str = "module ApplicationHelper\n  def icon(key, size: \"md\", **opts)\n    extra = opts.delete(:class)\n    sizes = { sm: \"w-4 h-4\", md: \"w-5 h-5\" }\n    glyph_tag(key, class: class_names(\"shrink-0\", sizes[size.to_sym], extra), **opts)\n  end\nend\n";

const UI_BASE: &str = "class Ui::BaseButton < ViewComponent::Base\n  TONES = { solid: \"bg-ink text-paper\", quiet: \"text-ink\" }.freeze\n\n  attr_reader :tone, :href, :icon, :text, :extra_classes, :opts\n\n  def initialize(tone: :solid, href: nil, text: nil, icon: nil, **opts)\n    @tone = tone.to_sym\n    @href = href\n    @text = text\n    @icon = icon\n    @extra_classes = opts.delete(:class)\n    @opts = opts\n  end\n\n  def classes\n    class_names(\"btn\", TONES[tone], extra_classes, icon ? \"btn-icon\" : nil)\n  end\nend\n";

const UI_BUTTON: &str = "class Ui::Button < Ui::BaseButton\n  attr_reader :confirm\n\n  def initialize(confirm: nil, **opts)\n    super(**opts)\n    @confirm = confirm\n  end\n\n  def container(&block)\n    data = confirm ? { turbo_confirm: confirm } : {}\n    if href\n      button_to(href, **opts.merge(class: classes, data: data), &block)\n    else\n      content_tag(:button, **opts.merge(class: classes, data: data), &block)\n    end\n  end\nend\n";

const UI_BUTTON_TEMPLATE: &str = "<%= container do %>\n  <% if icon %><%= helpers.icon(icon, size: :sm, class: \"fg-ink\") %><% end %>\n  <span><%= text %></span>\n<% end %>\n";

const UI_LINK_TEMPLATE: &str = "<%= link_to href, class: classes, **opts do %>\n  <% if icon %><%= helpers.icon(icon) %><% end %>\n  <%= text %>\n<% end %>\n";

/// A template its class body writes (`erb_template`), a lambda slot whose `content_tag` takes a label and the slot's
/// block, and a slot rendered by a component class.
const UI_STRIP: &str = "class Ui::Strip < ViewComponent::Base\n  erb_template <<~ERB\n    <%= tag.nav class: \"strip\" do %>\n      <% tabs.each do |tab| %>\n        <%= tab %>\n      <% end %>\n      <% links.each do |link| %>\n        <%= link %>\n      <% end %>\n    <% end %>\n  ERB\n\n  renders_many :tabs, ->(id:, label:, &block) do\n    content_tag(:button, label, id: id, type: \"button\", class: (id == active ? \"on\" : \"off\"), &block)\n  end\n\n  renders_many :links, Ui::Link\n\n  attr_reader :active\n\n  def initialize(active:)\n    @active = active\n  end\nend\n";

/// A slot lambda that renders a component and `safe_join`s escaped and safe parts, a `tag.<name>` whose options are
/// a literal Hash and a splat with a block, and a `header` slot beside another component's (the gem's slot registry
/// is each class's own).
const UI_SHEET: &str = "class Ui::Sheet < ViewComponent::Base\n  erb_template <<~ERB\n    <%= tag.section class: \"sheet\", **wrapper_opts do %>\n      <%= header %>\n      <% tabs.each do |tab| %><%= tab %><% end %>\n    <% end %>\n  ERB\n\n  renders_one :header, ->(title:, note: nil) do\n    content_tag(:header, class: \"panel-head\") do\n      close = render Ui::Button.new(text: \"x\", tone: :quiet)\n      safe_join([content_tag(:h2, title), note, close].compact, \" \")\n    end\n  end\n\n  renders_many :tabs, ->(label:) do\n    content_tag(:span, label, class: \"tab\")\n  end\n\n  def wrapper_opts\n    { data: { controller: \"sheet\" }, id: \"p1\" }\n  end\nend\n";

const UI_REPORT: &str = "class UiReport\n  def self.lines\n    view = ActionView::Base.new\n    [\n      Ui::Button.new(text: \"Save <now>\", tone: :quiet).render_in(view),\n      Ui::Button.new(text: \"Add\", icon: \"plus\", confirm: \"Sure?\", class: \"btn\").render_in(view),\n      Ui::Link.new(text: \"Home\", href: \"/home\", icon: \"minus\").render_in(view),\n      Ui::Button.new(text: \"Drop\", href: \"/items/1\", method: :delete, confirm: \"Gone?\").render_in(view),\n      Ui::Strip.new(active: \"a\").render_in(view) do |s|\n        s.with_tab(id: \"a\", label: \"One <1>\")\n        s.with_tab(id: \"b\", label: \"Two\") { \"2 > 1\" }\n        s.with_link(text: \"Out\", href: \"/out\", tone: :quiet)\n      end,\n      Ui::Sheet.new.render_in(view) do |p|\n        p.with_header(title: \"Edit <it>\", note: \"a & b\")\n        p.with_tab(label: \"Only\")\n      end\n    ].map(&:to_s)\n  end\nend\n";
