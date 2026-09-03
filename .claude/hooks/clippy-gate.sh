#!/usr/bin/env bash
# Stop / SubagentStop: refuse to stop while clippy reports warnings.
# Inert until Cargo.toml exists. Guards against the re-entry loop with stop_hook_active.
set -u
input="$(cat)"
if [ "$(printf '%s' "${input}" | jq -r '.stop_hook_active // false')" = "true" ]; then
	exit 0
fi
cd "${CLAUDE_PROJECT_DIR:-.}" || exit 0
[ -f Cargo.toml ] || exit 0
if out="$(cargo clippy --all-targets --locked -- -D warnings 2>&1)"; then
	exit 0
fi
tail="$(printf '%s\n' "${out}" | tail -n 40)"
jq -n --arg reason "clippy が警告を報告している。直してから終了すること:
${tail}" '{decision: "block", reason: $reason}'
exit 0
