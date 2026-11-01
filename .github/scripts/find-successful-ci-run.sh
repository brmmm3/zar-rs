#!/usr/bin/env bash
set -euo pipefail

if ! runs="$(gh api \
    "repos/${GITHUB_REPOSITORY}/actions/workflows/ci.yml/runs?head_sha=${GITHUB_SHA}&status=completed&per_page=100")"; then
    echo "::warning::Could not query prior CI runs; this workflow will build binaries from source"
    printf 'run_id=\n' >> "$GITHUB_OUTPUT"
    exit 0
fi

run_id="$(jq -r --arg sha "$GITHUB_SHA" '
    [.workflow_runs[]
        | select(
            .head_sha == $sha
            and .head_branch == "master"
            and (.event == "push" or .event == "workflow_dispatch")
            and .conclusion == "success"
        )]
    | sort_by(.run_number)
    | last
    | .id // empty
' <<< "$runs")"

printf 'run_id=%s\n' "$run_id" >> "$GITHUB_OUTPUT"
if [[ -n "$run_id" ]]; then
    echo "Reusing artifacts from successful CI run $run_id for $GITHUB_SHA"
else
    echo "No successful master CI run found for $GITHUB_SHA; builds will run locally"
fi
