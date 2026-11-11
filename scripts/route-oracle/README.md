# Route oracle

Boots a Rails app with its own locked Rails, prints `Rails.application.routes`,
and compares that table with the routes roundhouse reads from the same source
(`dump_routes`). It checks the route evaluator against Rails. It is never an
input to the compiler, and it does not run in CI: one app takes several
minutes and needs the app's whole bundle and services.

## Running it on an app

The app needs a working bundle (`bundle install` with its native libraries) and
whatever its boot reads (a database it can connect to, the environment keys
its initializers fetch).

```sh
APP=/absolute/path/to/app
RH=/absolute/path/to/roundhouse
OUT=/absolute/path/to/oracle-output
mkdir -p "$OUT"

# 1. Rails' table, from inside the app. A file, not stdout, which boot logging shares.
(cd "$APP" && ORACLE_OUT="$OUT/rails.txt" bundle exec rails runner "$RH/scripts/route-oracle/rails_routes.rb")

# 2. roundhouse's table. Install only the gem family being worked on into a separate
#    GEM_PATH, and move the app's `.bundle/config` aside while this runs: with
#    BUNDLE_PATH set (in that file or the environment), roundhouse reads every gem
#    of the bundle from source.
G="$OUT/source-gems"
for g in devise warden orm_adapter responders; do
  v=$(sed -n "s/^    $g (\([^)]*\))$/\1/p" "$APP/Gemfile.lock" | head -1)
  [ -n "$v" ] && gem install --install-dir "$G" --no-document --ignore-dependencies "$g" -v "$v"
done
(cd "$RH" && cargo build --release --bin dump_routes)
mv "$APP/.bundle/config" "$APP/.bundle/config.oracle"
env -u GEM_HOME -u BUNDLE_PATH GEM_PATH="$G" "$RH/target/release/dump_routes" --continue "$APP" > "$OUT/roundhouse.txt"
mv "$APP/.bundle/config.oracle" "$APP/.bundle/config"

# 3. Compare.
ORACLE_OUT_OF_SCOPE='[Oo]mniauth|/auth/:provider|Doorkeeper::|^/oauth/' \
  ruby "$RH/scripts/route-oracle/compare.rb" "$OUT/rails.txt" "$OUT/roundhouse.txt"
```

`ORACLE_OUT_OF_SCOPE` is a regexp over `PATH ENDPOINT`. Routes that match it
are left out on both sides; the printed "out of scope" count is Rails' routes
only. Use it for gems that are not read from
source yet, so their routes do not show up as differences.

## Reading the result

The first lines count each side: routes Rails serves, routes roundhouse serves,
and routes roundhouse refuses, by reason. Rails' own engines (`/rails/*`) and
out-of-scope routes are counted apart.

The lists that follow:

- **missing**: Rails serves the route and roundhouse does not. `[refused here]`
  means roundhouse reported the route as unsupported rather than dropping it.
- **extra**: roundhouse serves a route Rails does not.
- **different endpoint**: both sides have the route, but with different
  controller actions.
- **helpers missing / at another path**: route names (the `_path` helpers)
  that Rails defines and roundhouse does not, or defines with another path.

A route is its verb, its path's shape (segment names ignored) and its
controller action. Order counts only among routes of the same verb and shape,
where the first one wins.
