use super::emit_and_run;

#[test]
fn active_storage_current_set_restores_request_state() {
    emit_and_run::real_blog()
        .run_ruby(
            r#"original = { host: "original.example", protocol: "https" }
temporary = { only_path: true }
ActiveStorage::Current.url_options = original

result = ActiveStorage::Current.set(url_options: temporary) do
  raise "set did not enter the temporary state" unless ActiveStorage::Current.url_options == temporary
  :completed
end
raise "set changed the block result" unless result == :completed
raise "set did not restore after success" unless ActiveStorage::Current.url_options == original

begin
  ActiveStorage::Current.set(url_options: temporary) do
    raise "expected block error"
  end
rescue RuntimeError => error
  raise unless error.message == "expected block error"
end
raise "set did not restore after error" unless ActiveStorage::Current.url_options == original

Fiber.new do
  raise "request state was not shared within the thread" unless ActiveStorage::Current.url_options == original
  ActiveStorage::Current.set(url_options: temporary) do
    raise "fiber did not enter its state" unless ActiveStorage::Current.url_options == temporary
  end
  raise "fiber state did not restore" unless ActiveStorage::Current.url_options == original
end.resume
raise "fiber changed the caller's state" unless ActiveStorage::Current.url_options == original

Thread.new do
  raise "request state leaked across threads" unless ActiveStorage::Current.url_options.nil?
  ActiveStorage::Current.set(url_options: temporary) do
    raise "thread did not enter its state" unless ActiveStorage::Current.url_options == temporary
  end
  raise "thread state did not restore" unless ActiveStorage::Current.url_options.nil?
end.value
raise "thread changed the caller's state" unless ActiveStorage::Current.url_options == original

puts "ActiveStorage::Current.set state contract passed"
"#,
        )
        .assert_passes();
}

#[test]
fn active_storage_current_base_url_accepts_integer_port() {
    emit_and_run::real_blog()
        .run_ruby(
            r#"ActiveStorage::Current.url_options = {
  host: "example.test",
  protocol: "https",
  port: 3000,
}
url = ActiveStorage::Current.base_url
raise "unexpected base_url: #{url.inspect}" unless url == "https://example.test:3000"

ActiveStorage::Current.set(url_options: { host: "local.test", protocol: "http", port: 8080 }) do
  nested = ActiveStorage::Current.base_url
  raise "unexpected nested base_url: #{nested.inspect}" unless nested == "http://local.test:8080"
end

puts "ActiveStorage::Current.base_url integer port contract passed"
"#,
        )
        .assert_passes();
}

const PORT_AND_SET_SCRIPT: &str = r##"ActiveStorage::Current.url_options = { protocol: "https", host: "a.test", port: 443 }
raise "standard https port kept: #{ActiveStorage::Current.base_url}" unless ActiveStorage::Current.base_url == "https://a.test"
ActiveStorage::Current.url_options = { protocol: "http://", host: "a.test", port: 80 }
raise "standard http port kept: #{ActiveStorage::Current.base_url}" unless ActiveStorage::Current.base_url == "http://a.test"
ActiveStorage::Current.url_options = { protocol: "http", host: "a.test", port: nil }
raise "nil port: #{ActiveStorage::Current.base_url}" unless ActiveStorage::Current.base_url == "http://a.test"
inner = ActiveStorage::Current.set(url_options: { protocol: "http", host: "b.test", port: 3000 }) do
  ActiveStorage::Current.base_url
end
raise "set block value: #{inner}" unless inner == "http://b.test:3000"
raise "set did not restore: #{ActiveStorage::Current.base_url}" unless ActiveStorage::Current.base_url == "http://a.test"
puts "ok"
"##;

#[test]
fn active_storage_current_omits_a_standard_port() {
    emit_and_run::real_blog().run_ruby(PORT_AND_SET_SCRIPT).assert_passes();
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn active_storage_current_port_and_set_on_spinel() {
    emit_and_run::real_blog().run_spinel(PORT_AND_SET_SCRIPT).assert_passes();
}
