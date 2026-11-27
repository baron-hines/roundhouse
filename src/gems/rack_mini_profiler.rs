//! rack-mini-profiler on the Spinel tree: `runtime/gems/rack_mini_profiler/`.

pub(super) const PACKAGE: super::GemPackage = super::GemPackage {
    gem: "rack-mini-profiler",
    dir: "rack_mini_profiler",
    entry: "rack_mini_profiler",
    config_receiver: Some("Rack::MiniProfiler.config"),
    // The writers `mini_profiler/config.rb` defines and reads; another of the gem's settings would be a NoMethodError at boot.
    config_setters: &[
        "authorization_mode", "auto_inject", "base_url_path", "cookie_path", "disable_caching", "enabled",
        "skip_paths", "collapse_results", "max_traces_to_show", "position", "show_children", "show_controls",
        "show_trivial", "show_total_sql_count", "start_hidden", "toggle_shortcut", "html_container",
        "enable_hotwire_turbo_drive_support", "profile_parameter",
    ],
};
