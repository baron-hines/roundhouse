//! `dump_routes` — the routes roundhouse reads from a Rails app, one per line, in the shape
//! `scripts/route-oracle/rails_routes.rb` prints Rails' own: `VERB PATH Controller#action NAME STATE`.
//! `scripts/route-oracle/compare.rb` lines the two up.
//!
//!     cargo run --bin dump_routes -- [--continue] APP
//!
//! STATE is `dispatched`, `guarded` (the Ruby family asks a request constraint first) or
//! `refused:<why>` (left out of every dispatch table, its URL helper still generated).

use roundhouse::dialect::HttpMethod;

fn main() {
    roundhouse::stack::run(|| {
        let args: Vec<String> = std::env::args().skip(1).collect();
        let surveying = args.iter().any(|a| a == "--continue");
        let Some(dir) = args.iter().find(|a| !a.starts_with("--")) else {
            eprintln!("usage: dump_routes [--continue] APP");
            std::process::exit(2);
        };
        if surveying {
            roundhouse::ingest::survey::activate();
        }
        let app = match roundhouse::ingest::ingest_app(std::path::Path::new(dir)) {
            Ok(app) => app,
            Err(err) => {
                eprintln!("dump_routes: ingest failed: {err}");
                std::process::exit(1);
            }
        };
        if surveying {
            let gaps = roundhouse::ingest::survey::drain();
            eprint!("{}", roundhouse::ingest::survey::render_report(&gaps));
        }
        let guarded: Vec<(String, String)> = roundhouse::lower::routes::flatten_routes_asking_guards(&app)
            .into_iter()
            .filter(|(_, guards)| !guards.is_empty())
            .map(|(r, _)| (verb(r.method).to_string(), r.path))
            .collect();
        for route in roundhouse::lower::routes::flatten_routes_for_helpers(&app) {
            let verb = verb(route.method);
            let state = match &route.refused {
                None => "dispatched".to_string(),
                Some(_) if guarded.iter().any(|(v, p)| v == verb && *p == route.path) => "guarded".to_string(),
                Some((why, _)) => format!("refused:{}", why.replace(' ', "_")),
            };
            let name = if route.named { route.as_name.as_str() } else { "-" };
            let class = route.controller.0.as_str();
            let endpoint = if class == "RoundhouseRedirectsController" { "REDIRECT".to_string() } else { format!("{class}#{}", route.action) };
            println!("{verb} {} {endpoint} {name} {state}", route.path);
        }
    })
}

fn verb(method: HttpMethod) -> &'static str {
    match method {
        HttpMethod::Get => "GET",
        HttpMethod::Post => "POST",
        HttpMethod::Put => "PUT",
        HttpMethod::Patch => "PATCH",
        HttpMethod::Delete => "DELETE",
        HttpMethod::Head => "HEAD",
        HttpMethod::Options => "OPTIONS",
        HttpMethod::Any => "ANY",
    }
}
