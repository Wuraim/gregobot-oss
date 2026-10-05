#!/usr/bin/env bash
# Enforce master merge nomenclature: "MAJOR.MINOR.PATCH: description"
# Usage: check_commit_message.sh ["full commit message or PR title"]
#        COMMIT_MESSAGE="..." check_commit_message.sh
set -euo pipefail

PATTERN='^[0-9]+\.[0-9]+\.[0-9]+: .+'

subject="${1:-${COMMIT_MESSAGE:-}}"
if [[ -z "${subject}" ]]; then
  echo "usage: check_commit_message.sh <message>" >&2
  exit 2
fi

# Only the first line counts (git subject / PR title).
subject="${subject%%$'\n'*}"
subject="${subject%"${subject##*[![:space:]]}"}"

if [[ "${subject}" =~ $PATTERN ]]; then
  exit 0
fi

echo "Commit/PR subject must match: MAJOR.MINOR.PATCH: description" >&2
echo "  got: ${subject}" >&2
echo "  example: 0.24.5: Harden Gemini tool loop" >&2
exit 1
