//! discourse's `%w[users u].each_with_index do |root_path, index|`: the loop draws its routes once per
//! element, and `get(**{ path => to }.merge(index == 1 ? { as: … } : {}))` names a route on one pass only.

use super::emit_and_run;

const ROUTES: &str = r##"  %w[users u].each_with_index do |root_path, index|
    get "#{root_path}/random-username" => "articles#index"
    get(
      **{ "#{root_path}/account-created/" => "articles#index" }.merge(
        index == 1 ? { as: :users_account_created } : { as: :old_account_created },
      ),
    )
    get(
      **{
        "#{root_path}/activate-account/:token" => "articles#index",
        :constraints => {
          token: /[0-9a-f]+/,
        },
      }.merge(index == 1 ? { as: "perform_activate_account" } : {}),
    )
  end
"##;

#[test]
fn an_indexed_route_loop_draws_each_pass_and_names_by_the_index() {
    let run = emit_and_run::real_blog()
        .edit("config/routes.rb", "  root \"articles#index\"\n", &format!("  root \"articles#index\"\n{ROUTES}"))
        .run_ruby(
            r##"
def hit(verb, path)
  env = { "REQUEST_METHOD" => verb, "PATH_INFO" => path, "QUERY_STRING" => "", "REMOTE_ADDR" => "127.0.0.1",
          "HTTP_HOST" => "localhost", "rack.input" => StringIO.new("") }
  Main.run_rack(env)[0]
end
{ ["GET", "/users/random-username"] => 200, ["GET", "/u/random-username"] => 200,
  ["GET", "/users/account-created"] => 200, ["GET", "/u/account-created"] => 200,
  ["GET", "/u/activate-account/abc1"] => 200, ["GET", "/u/activate-account/zzz"] => 404 }.each do |(verb, path), want|
  got = hit(verb, path)
  raise "#{verb} #{path}: #{got}" unless got == want
end
names = [RouteHelpers.old_account_created_path, RouteHelpers.users_account_created_path, RouteHelpers.perform_activate_account_path("ab")]
raise names.inspect unless names == ["/users/account-created", "/u/account-created", "/u/activate-account/ab"]
puts "route loop passed"
"##,
        );
    run.assert_passes();
    assert!(run.stdout.contains("route loop passed"), "{}", run.stdout);
}
