//! A class that `include`s REXML and names its classes bare, as svg-graph
//! does, shared by the Ruby lane and the Spinel lane. Ruby finds `Comment`
//! in the included module before `Object`, so real-blog's own `Comment`
//! model is not the one built. The answers are CRuby 3.4's with rexml 3.4.

use super::emit_and_run;

pub const SVG_DOC: &str = r##"class SvgDoc
  include REXML

  def self.render
    doc = Document.new
    doc << XMLDecl.new
    root = doc.add_element("svg", { "width" => "10", "xmlns" => "http://www.w3.org/2000/svg" })
    root << Comment.new(" made here ")
    style = root.add_element("style", { "type" => "text/css" })
    style << CData.new("rect { fill: red }")
    rect = root.add_element("rect", { "x" => "1" })
    rect.text = "a < b"
    doc.to_s
  end

  def self.parsed_name
    REXML::Document.new("<a><b n='1'/></a>").root.elements[1].attributes["n"]
  end
end
"##;

/// A DocType and REXML's pretty formatter, as svg-graph writes its page
/// (`doc.write(data, 0)`, `Formatters::Pretty` with `compact`), into a
/// bare `""` buffer as svg-graph does (see `mutable_literals`).
pub const SVG_PAGE: &str = r##"class SvgPage
  include REXML

  def self.render
    doc = Document.new
    doc << XMLDecl.new
    doc << DocType.new(%q{svg PUBLIC "-//W3C//DTD SVG 1.0//EN" "http://www.w3.org/TR/2001/REC-SVG-20010904/DTD/svg10.dtd"})
    root = doc.add_element("svg", { "width" => "10" })
    root.add_element("rect", { "x" => "1" }).text = "label"
    data = ""
    doc.write(data, 0)
    data
  end

  def self.compact
    root = Element.new("g")
    root.add_element("text").text = "a"
    f = REXML::Formatters::Pretty.new(0)
    f.compact = true
    out = ""
    f.write(root, out)
    out
  end
end
"##;

/// Names REXML only through `include REXML`: the require is still written.
pub const ONLY_INCLUDE: &str = r##"class OnlyInclude
  include REXML

  def self.build
    doc = Document.new
    doc.add_element("a")
    doc.to_s
  end
end
"##;

pub const ASSERTIONS: &str = r##"want = "<?xml version='1.0'?><svg width='10' xmlns='http://www.w3.org/2000/svg'><!-- made here --><style type='text/css'><![CDATA[rect { fill: red }]]></style><rect x='1'>a &lt; b</rect></svg>"
got = SvgDoc.render
raise "render #{got.inspect}" unless got == want
raise "parsed" unless SvgDoc.parsed_name == "1"
raise "page #{SvgPage.render.inspect}" unless SvgPage.render == "<?xml version='1.0'?>\n<!DOCTYPE svg PUBLIC \"-//W3C//DTD SVG 1.0//EN\" \"http://www.w3.org/TR/2001/REC-SVG-20010904/DTD/svg10.dtd\">\n<svg width='10'>\n<rect x='1'>\nlabel\n</rect>\n</svg>"
raise "compact #{SvgPage.compact.inspect}" unless SvgPage.compact == "<g>\n<text>a</text>\n</g>"
raise "only include" unless OnlyInclude.build == "<a/>"
puts "rexml passed"
"##;

#[allow(dead_code)]
pub fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog().write("app/models/svg_doc.rb", SVG_DOC).write("app/models/svg_page.rb", SVG_PAGE)
        .write("app/models/only_include.rb", ONLY_INCLUDE)
}
