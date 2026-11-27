#[path = "support/emit_and_run.rs"]
#[allow(dead_code)]
mod emit_and_run;

const CONTRACT: &str = r##"require_relative "runtime/multipart"
require_relative "runtime/cgi_io"
boundary = "parts-test"
body = "--parts-test\r\nContent-Disposition: form-data; name=\"prompt[attachments][]\"; filename=\"first.jpg\"\r\nContent-Type: image/jpeg\r\n\r\n\xFF\x00first\r\n--parts-test\r\nContent-Disposition: form-data; name=\"prompt[attachments][]\"; filename=\"second.jpg\"\r\nContent-Type: image/jpeg\r\n\r\nsecond\r\n--parts-test--\r\n".b
form = ActionDispatch::Http::Multipart.parse(body, "multipart/form-data; boundary=#{boundary}")
raise "parts" unless form.file_parts.length == 2
raise "first" unless form.file_parts[0].file.original_filename == "first.jpg"
raise "bytes" unless form.file_parts[0].file.read == "\xFF\x00first".b
raise "last" unless form.files["prompt[attachments][]"].original_filename == "second.jpg"
params = {}
form.file_parts.each { |part| CgiIo.assign_form_pair(params, part.name, part.file) }
files = params["prompt"]["attachments"]
raise "array" unless files.is_a?(Array) && files.length == 2
raise "order" unless files.map(&:original_filename) == ["first.jpg", "second.jpg"]
puts "PASS"
"##;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("db/schema.rb", "ActiveRecord::Schema[8.1].define(version: 1) do\n  create_table :widgets do |t|\n    t.string :name\n  end\nend\n")
        .write("app/models/application_record.rb", "class ApplicationRecord < ActiveRecord::Base\nend\n")
        .write("app/models/widget.rb", "class Widget < ApplicationRecord\nend\n")
}

#[test]
fn repeated_multipart_files_keep_order_and_binary_bytes() {
    app().run_ruby(CONTRACT).assert_passes();
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn repeated_file_parts_run_with_emitted_rbs() {
    app().run_spinel_with_rbs(CONTRACT).assert_passes();
}
