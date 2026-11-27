use super::emit_and_run;

const END_REQUEST: &str = r#"class EndRequest
  include ActiveModel::Model
  include ActiveModel::Attributes

  attribute :pty_session_id, :integer
  attribute :remove_workspace, :boolean, default: false
  attribute :discard_changes, :boolean, default: false
  attribute :base, :string, default: "main"

  validates :pty_session_id, presence: true
  validate :base_is_present, if: :remove_workspace

  def base_is_present
    errors.add(:base, "is required") if base.blank?
  end
end
"#;

fn forms() -> emit_and_run::Overlay {
    let mut app = emit_and_run::real_blog().write("app/models/end_request.rb", END_REQUEST);
    for (resource, class_name, attributes) in [
        ("resume_request", "ResumeRequest", ""),
        (
            "merge_request",
            "MergeRequest",
            "  attribute :base, :string\n  attribute :message, :string\n  validates :message, presence: true\n",
        ),
    ] {
        app = app.write(
            &format!("app/models/{resource}.rb"),
            &format!("class {class_name}\n  include ActiveModel::Model\n  include ActiveModel::Attributes\n\n  attribute :pty_session_id, :integer\n{attributes}  validates :pty_session_id, presence: true\nend\n"),
        );
    }
    let mut routes = String::new();
    for (resource, class_name, fields) in [
        ("resume_request", "ResumeRequest", "pty_session_id"),
        (
            "end_request",
            "EndRequest",
            "pty_session_id remove_workspace discard_changes base",
        ),
        (
            "merge_request",
            "MergeRequest",
            "pty_session_id base message",
        ),
    ] {
        for (prefix, module) in [("", ""), ("api/", "Api::")] {
            app = app.write(
                &format!("app/controllers/{prefix}{resource}s_controller.rb"),
                &format!("class {module}{class_name}sController < ApplicationController\n  def create\n    form = {class_name}.new(request_params)\n    render json: {{ id: form.pty_session_id, valid: form.valid? }}\n  end\n\n  private\n\n  def request_params\n    params.expect({resource}: %i[{fields}])\n  end\nend\n"),
            );
            routes.push_str(&format!(
                "  post \"/{prefix}{resource}s\", to: \"{prefix}{resource}s#create\"\n"
            ));
        }
    }
    app.edit(
        "config/routes.rb",
        "Rails.application.routes.draw do\n",
        &format!("Rails.application.routes.draw do\n{routes}"),
    )
}

const FACTORIES: &str = r##"
def check(label, actual, expected)
  raise "#{label}: #{actual.inspect}, expected #{expected.inspect}" unless actual == expected
end
resume = ResumeRequest.from_params(ResumeRequestParams.from_raw({"resume_request" => {"pty_session_id" => "12"}}))
check("resume id", resume.pty_session_id, 12)
check("resume valid", resume.valid?, true)
merge = MergeRequest.from_params(MergeRequestParams.from_raw({"merge_request" => {"pty_session_id" => "12", "base" => "main", "message" => "Merge branch"}}))
check("merge id", merge.pty_session_id, 12)
check("merge message", merge.message, "Merge branch")
check("merge valid", merge.valid?, true)
request = EndRequest.from_params(EndRequestParams.from_raw({"end_request" => {"pty_session_id" => "12", "remove_workspace" => "1", "discard_changes" => "off", "base" => "topic"}}))
check("end id", request.pty_session_id, 12)
check("end flag", request.remove_workspace, true)
check("end other flag", request.discard_changes, false)
check("end base", request.base, "topic")
check("end valid", request.valid?, true)
omitted = EndRequest.from_params(EndRequestParams.from_raw({"end_request" => {"pty_session_id" => "12"}}))
check("omitted flag", omitted.remove_workspace, false)
check("omitted base", omitted.base, "main")
blank = EndRequest.from_params(EndRequestParams.from_raw({"end_request" => {"pty_session_id" => "", "remove_workspace" => "", "base" => ""}}))
check("blank integer", blank.pty_session_id, nil)
check("blank boolean", blank.remove_workspace, nil)
check("blank string", blank.base, "")
check("invalid blank id", blank.valid?, false)
invalid = EndRequest.from_params(EndRequestParams.from_raw({"end_request" => {"pty_session_id" => "12", "remove_workspace" => "1", "base" => ""}}))
check("conditional validation", invalid.valid?, false)
puts "tableless params factories passed"
"##;

#[test]
fn tableless_factories_cast_values_and_keep_omitted_defaults() {
    forms().run_ruby(FACTORIES).assert_passes();
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn spinel_gate_tableless_factories_cast_values_and_keep_omitted_defaults() {
    forms().run_spinel(FACTORIES).assert_passes();
}

#[test]
fn browser_and_api_controllers_construct_tableless_forms() {
    forms().run_ruby(r##"
require "stringio"
ActionController::Base.allow_forgery_protection = false
def post(path, body)
  env = {
    "REQUEST_METHOD" => "POST", "PATH_INFO" => path, "QUERY_STRING" => "",
    "CONTENT_TYPE" => "application/x-www-form-urlencoded",
    "CONTENT_LENGTH" => body.bytesize.to_s, "HTTP_ACCEPT" => "application/json"
  }
  status, response = Main.dispatch_core(env, StringIO.new(body))
  raise "#{path}: #{status}" unless status == 200
  JSON.parse(response)
end
["", "/api"].each do |prefix|
  %w[resume_request end_request merge_request].each do |resource|
    body = "#{resource}[pty_session_id]=12&#{resource}[message]=Merge"
    response = post("#{prefix}/#{resource}s", body)
    raise response.inspect unless response == {"id" => 12, "valid" => true}
    response = post("#{prefix}/#{resource}s", "#{resource}[pty_session_id]=&#{resource}[message]=Merge")
    raise response.inspect unless response == {"id" => nil, "valid" => false}
  end
end
puts "six tableless controller sites passed"
"##).assert_passes();
}

#[test]
fn tableless_factories_keep_distinct_permit_lists_and_filter_writers() {
    forms()
        .write("app/models/end_request.rb", &END_REQUEST.replace("\nend\n", r#"
  attr_reader :note

  def note=(value)
    @note = value.upcase
  end

  private

  def secret=(value)
    raise "private writer called"
  end
end
"#))
        .write("app/controllers/previews_controller.rb", r#"class PreviewsController < ApplicationController
  def create
    form = EndRequest.new(preview_params)
    render json: { id: form.pty_session_id }
  end

  private

  def preview_params
    params.expect(end_request: %i[pty_session_id note secret ghost])
  end
end
"#)
        .run_ruby(r#"
require "stringio"
ActionController::Base.allow_forgery_protection = false
params = PreviewsEndRequestParams.from_raw({"end_request" => {"pty_session_id" => "21", "remove_workspace" => "1", "base" => "private", "note" => "preview", "secret" => "hidden", "ghost" => "missing"}})
preview = EndRequest.from_previews_end_request_params(params)
raise preview.pty_session_id.inspect unless preview.pty_session_id == 21
raise preview.remove_workspace.inspect unless preview.remove_workspace == false
raise preview.base.inspect unless preview.base == "main"
raise preview.note.inspect unless preview.note == "PREVIEW"
puts "distinct tableless permit lists passed"
"#).assert_passes();
}

#[test]
fn tableless_factory_filters_a_private_override_of_an_accessor_writer() {
    forms()
        .write("app/models/end_request.rb", &END_REQUEST.replace("\nend\n", r#"
  attr_accessor :note, :public_note

  private

  def note=(value)
    raise "private accessor override called"
  end
end
"#))
        .write("app/controllers/previews_controller.rb", r#"class PreviewsController < ApplicationController
  def create
    form = EndRequest.new(preview_params)
    render json: { id: form.pty_session_id }
  end

  private

  def preview_params
    params.expect(end_request: %i[pty_session_id note public_note])
  end
end
"#)
        .run_ruby(r#"
params = PreviewsEndRequestParams.from_raw({"end_request" => {"pty_session_id" => "21", "note" => "hidden", "public_note" => "visible"}})
preview = EndRequest.from_previews_end_request_params(params)
raise preview.pty_session_id.inspect unless preview.pty_session_id == 21
raise preview.note.inspect unless preview.note.nil?
raise preview.public_note.inspect unless preview.public_note == "visible"
puts "private accessor override filtered"
"#).assert_passes();
}

fn forms_with_source_factories() -> emit_and_run::Overlay {
    forms()
        .write(
            "app/models/end_request.rb",
            &END_REQUEST.replace(
                "\nend\n",
                r#"
  def self.from_params(params)
    request = new
    request.pty_session_id = params.pty_session_id.to_i + 100
    request.base = "canonical source"
    request
  end

  def self.from_previews_end_request_params(params)
    request = new
    request.pty_session_id = params.pty_session_id.to_i + 200
    request.base = "named source"
    request
  end
end
"#,
            ),
        )
        .write(
            "app/controllers/previews_controller.rb",
            r#"class PreviewsController < ApplicationController
  def create
    form = EndRequest.new(preview_params)
    render json: { id: form.pty_session_id }
  end

  private

  def preview_params
    params.expect(end_request: %i[pty_session_id base])
  end
end
"#,
        )
}

#[test]
fn tableless_factory_preserves_a_source_from_params_method() {
    forms_with_source_factories()
        .run_ruby(
            r#"
params = EndRequestParams.from_raw({"end_request" => {"pty_session_id" => "12"}})
request = EndRequest.from_params(params)
raise request.pty_session_id.inspect unless request.pty_session_id == 112
raise request.base.inspect unless request.base == "canonical source"
puts "canonical source factory preserved"
"#,
        )
        .assert_passes();
}

#[test]
fn tableless_factory_preserves_a_source_named_factory_method() {
    forms_with_source_factories()
        .run_ruby(
            r#"
params = PreviewsEndRequestParams.from_raw({"end_request" => {"pty_session_id" => "21"}})
request = EndRequest.from_previews_end_request_params(params)
raise request.pty_session_id.inspect unless request.pty_session_id == 221
raise request.base.inspect unless request.base == "named source"
puts "named source factory preserved"
"#,
        )
        .assert_passes();
}

#[test]
fn tableless_factory_refuses_unconvertible_multiparameter_input_before_new() {
    forms()
        .run_ruby(
            r#"
def EndRequest.new
  raise "constructor reached before refusal"
end
params = EndRequestParams.from_raw({"end_request" => {"pty_session_id(1i)" => "invalid"}})
raise "unsupported flag missing" unless params.rh_multiparameter_unsupported
begin
  EndRequest.from_params(params)
  raise "unsupported input accepted"
rescue RuntimeError => error
  raise error.message unless error.message.include?("multiparameter key")
end
puts "unconvertible tableless multiparameter input refused before construction"
"#,
        )
        .assert_passes();
}

#[test]
fn tableless_factory_refuses_multiparameter_parts_without_a_rails_assignment_path() {
    forms()
        .run_ruby(
            r#"
def EndRequest.new
  raise "constructor reached before refusal"
end
[{"pty_session_id(1i)" => "12"}, {"base(1)" => "topic"}].each do |input|
  params = EndRequestParams.from_raw({"end_request" => input})
  raise "parts should convert" if params.rh_multiparameter_unsupported
  begin
    EndRequest.from_params(params)
    raise "multiparameter input accepted"
  rescue RuntimeError => error
    raise error.message unless error.message.include?("multiparameter key")
  end
end
puts "tableless multipart assignment refused before construction"
"#,
        )
        .assert_passes();
}
