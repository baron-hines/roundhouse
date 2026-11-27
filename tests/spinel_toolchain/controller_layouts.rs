//! Each action's layout over the native HTTP server; the Ruby-lane twin is
//! `tests/controller_layouts.rs`.

use super::controller_layouts_contract as contract;
use super::native_http;

#[test]
#[ignore = "requires Spinel; run in its CI lane"]
fn each_action_renders_in_the_layout_rails_picks_natively() {
    let tree = contract::blog().native_http_tree();
    let server = native_http::Server::start(&tree);
    for (path, query, status, body) in contract::PAGES {
        let url = if query.is_empty() { path.to_string() } else { format!("{path}?{query}") };
        let page = server.get(&url);
        let text = page.body.split_whitespace().collect::<Vec<_>>().join(" ");
        assert_eq!((page.status, text.as_str()), (*status, *body), "{url}\n{}", server.log());
    }
}
