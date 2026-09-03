#!/usr/bin/env bash
# PostToolUse (Write|Edit): format the crate after a .rs file is written.
# Inert until Cargo.toml exists. Never blocks the tool (exit 0 on any outcome).
set -u
file="$(jq -r '.tool_input.file_path // .tool_response.filePath // empty')"
case "${file}" in
	*.rs) ;;
	*) exit 0 ;;
esac
cd "${CLAUDE_PROJECT_DIR:-.}" || exit 0
run-if-present path Cargo.toml -- cargo fmt 2>/dev/null || true
exit 0
