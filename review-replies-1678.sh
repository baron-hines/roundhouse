#!/usr/bin/env bash
# Replies to CodeRabbit thread 4442158358 on #1678, resolves that thread (and no other),
# corrects the earlier outside-diff comment's claim, and refreshes the PR body.
# Run from spiff with Baron's gh login.
set -euo pipefail
R=roundhouse-rb/roundhouse
N=1678
FOOT=$'\n\n🤖 Generated with [Claude Code](https://claude.com/claude-code)'

gh api -X POST "repos/$R/pulls/$N/comments/4442158358/replies" -f body="Fixed in 76cfd5e0. A keyword no analyzed call site passes still has no codec, so it isn't written, but \`payload_<job>\` and \`push_<job>\` now raise \`ArgumentError\` when handed any value other than its literal default, so a call the analysis didn't see fails loudly instead of losing the value. tests/emit_and_run/job_keywords.rs checks both the default (not written) and another value (raises).$FOOT" --jq .html_url

# Correct the earlier comment's claim that such a keyword is always its default.
C=6587124654
OLD=$(gh api "repos/$R/issues/comments/$C" --jq .body)
NEW=${OLD/"Such a keyword is always its literal default, so it is never written and the reader passes the default."/"~~Such a keyword is always its literal default, so it is never written and the reader passes the default.~~ [Edit: a call the analysis doesn't see could pass another value; since 76cfd5e0 the writers raise \`ArgumentError\` for it instead of dropping it.]"}
[ "$NEW" != "$OLD" ] || { echo "comment text not found; not edited" >&2; exit 1; }
gh api -X PATCH "repos/$R/issues/comments/$C" -f body="$NEW" --jq .html_url

# Resolve only thread 4442158358.
gh api graphql -F n=$N -f query='query($n:Int!){repository(owner:"roundhouse-rb",name:"roundhouse"){pullRequest(number:$n){reviewThreads(first:100){nodes{id isResolved comments(first:1){nodes{databaseId}}}}}}}' \
  --jq '.data.repository.pullRequest.reviewThreads.nodes[] | select((.isResolved|not) and .comments.nodes[0].databaseId == 4442158358) | .id' |
while read -r t; do gh api graphql -f query="mutation{resolveReviewThread(input:{threadId:\"$t\"}){thread{isResolved}}}" --jq .data.resolveReviewThread.thread.isResolved; done

gh pr edit $N -R $R --body "$(gh api 'repos/baron-hines/roundhouse/contents/pr-body.md?ref=sidekiq-pr-body-kwargs' --jq .content | base64 -d)"
