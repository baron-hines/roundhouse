#!/usr/bin/env bash
# Replies to the two CodeRabbit comments on rubys/roundhouse#1678, resolves their threads,
# and refreshes the PR body. Run from spiff with Baron's gh login.
set -euo pipefail
R=rubys/roundhouse
N=1678
FOOT=$'\n\n🤖 Generated with [Claude Code](https://claude.com/claude-code)'
id_for() { # substring of the CodeRabbit comment's title
  gh api "repos/$R/pulls/$N/comments" --paginate \
    --jq ".[] | select(.user.login==\"coderabbitai[bot]\" and .in_reply_to_id==null and (.body|contains(\"$1\"))) | .id" | head -1
}
reply() { # comment_id text
  gh api -X POST "repos/$R/pulls/$N/comments/$1/replies" -f body="$2$FOOT" --jq .html_url
}
reply "$(id_for 'refused keyword-rest')" "Fixed in cb8313d9. A job refused a wrapper (a \`**\` keyword rest, a block, \`...\`, or a keyword default that isn't a literal) now gets an error at each \`perform_later\` / \`perform_now\` call site, and the declaration keeps its warning. \`calls_to_a_job_refused_a_wrapper_are_errors\` in tests/emit_and_run/job_keywords.rs covers it."
reply "$(id_for 'omitted keyword defaults')" "Fixed in cb8313d9. A \`perform\` whose keyword default isn't a literal (\`at: Time.now\`, a method call) now gets no wrapper and takes the residue diagnostic, so nothing is evaluated at enqueue time or on the class. A literal default is kept in the wrapper: it reads the same wherever and whenever it is evaluated, and the payload writer already omits a keyword at its literal default, which reads back as that default."
gh api graphql -F n=$N -f query='query($n:Int!){repository(owner:"rubys",name:"roundhouse"){pullRequest(number:$n){reviewThreads(first:50){nodes{id isResolved}}}}}' \
  --jq '.data.repository.pullRequest.reviewThreads.nodes[] | select(.isResolved|not) | .id' |
while read -r t; do gh api graphql -f query="mutation{resolveReviewThread(input:{threadId:\"$t\"}){thread{isResolved}}}" --jq .data.resolveReviewThread.thread.isResolved; done
gh pr edit $N -R $R --body "$(gh api 'repos/baron-hines/roundhouse/contents/pr-body.md?ref=sidekiq-pr-body-kwargs' --jq .content | base64 -d)"
