#!/usr/bin/env bash
# Replies to the five CodeRabbit threads on #1678 and its outside-diff comments, resolves those five threads (and no others),
# and refreshes the PR body. Run from spiff with Baron's gh login.
set -euo pipefail
R=roundhouse-rb/roundhouse
N=1678
FOOT=$'\n\n🤖 Generated with [Claude Code](https://claude.com/claude-code)'
id_for() { # substring of the CodeRabbit comment's title
  gh api "repos/$R/pulls/$N/comments" --paginate \
    --jq ".[] | select(.user.login==\"coderabbitai[bot]\" and .in_reply_to_id==null and (.body|contains(\"$1\"))) | .id" | head -1
}
reply() { # comment_id text
  gh api -X POST "repos/$R/pulls/$N/comments/$1/replies" -f body="$2$FOOT" --jq .html_url
}
reply 4436674709 "Fixed in cb8313d9. A job refused a wrapper (a \`**\` keyword rest, a block, \`...\`, or a keyword default that isn't a literal) now gets an error at each \`perform_later\` / \`perform_now\` call site, and the declaration keeps its warning. \`calls_to_a_job_refused_a_wrapper_are_errors\` in tests/emit_and_run/job_keywords.rs covers it."
reply 4436674713 "Fixed in cb8313d9. A \`perform\` whose keyword default isn't a literal (\`at: Time.now\`, a method call) now gets no wrapper and takes the residue diagnostic, so nothing is evaluated at enqueue time or on the class. A literal default is kept in the wrapper: it reads the same wherever and whenever it is evaluated, and the payload writer already omits a keyword at its literal default, which reads back as that default."
reply 4437631231 "Fixed in 2e79ce4c. \`keywords\` now answers \"\" when every keyword is left out, and \`list\` drops it, so a call with no keywords writes no Hash, as Rails does. A job whose keywords all have defaults reads with the new \`optional_keywords_at\`, which takes a missing Hash as every keyword absent, so a payload Rails enqueued decodes. The .rbs has it, and the activejob 8.1.3 oracle (tests/active_job_payload) now covers a call with no keywords (\`[gid]\`)."
reply 4437752969 "Fixed in d6e3a3ae. \`optional_keywords_at\` now takes the Hash as absent only when \`args.length == i\`. A shorter array falls through to \`keywords_at\`, which raises \`DeserializationError\`. tests/active_job_payload has a check for a too-short array."

reply 4438061399 "Not changing this one. A job gets a payload only when every call site passes every positional argument; a site that passes a different number keeps the Proc. So a payload this tree writes always has all of them, and \`optional_keywords_at\` sees the Hash exactly at \`i\` or not at all. Reading a payload that Rails wrote with optional positionals omitted is the arity contract the positional readers already had before this PR. It needs its own change, not one in this reader."

# The outside-diff comments of the latest review have no thread to reply in.
gh api -X POST "repos/$R/issues/$N/comments" -f body="On CodeRabbit's outside-diff comments:

- **Plan optional keywords that no call site supplies:** fixed in 1e7e5904. Such a keyword is always its literal default, so it is never written and the reader passes the default. tests/emit_and_run/job_keywords.rs covers it (\`tag:\`).
- **Escape Ruby interpolation markers in \`ruby_str\`:** fixed in 1e7e5904. \`#\` is escaped, and the same test's \`'#{tag}'\` default stays text.
- **Preserve an explicitly supplied default-valued keyword:** deliberate. The class-side wrapper can't tell an omitted keyword from one passed at its default, so both are left out. \`perform\` receives the same value either way, and runtime.md lists it as a known difference from Rails.$FOOT" --jq .html_url

# Resolve only the five threads answered above; any other thread stays open.
for c in 4436674709 4436674713 4437631231 4437752969 4438061399; do
  gh api graphql -F n=$N -f query='query($n:Int!){repository(owner:"roundhouse-rb",name:"roundhouse"){pullRequest(number:$n){reviewThreads(first:100){nodes{id isResolved comments(first:1){nodes{databaseId}}}}}}}' \
    --jq ".data.repository.pullRequest.reviewThreads.nodes[] | select((.isResolved|not) and .comments.nodes[0].databaseId == $c) | .id" |
  while read -r t; do gh api graphql -f query="mutation{resolveReviewThread(input:{threadId:\"$t\"}){thread{isResolved}}}" --jq .data.resolveReviewThread.thread.isResolved; done
done
gh pr edit $N -R $R --body "$(gh api 'repos/baron-hines/roundhouse/contents/pr-body.md?ref=sidekiq-pr-body-kwargs' --jq .content | base64 -d)"
