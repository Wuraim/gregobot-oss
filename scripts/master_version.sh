#!/usr/bin/env bash
# Read the latest MAJOR.MINOR.PATCH subject on a git ref and optionally bump it.
# Skips unversioned commits (feature-branch leftovers that landed on master).
#
# Usage:
#   master_version.sh latest [ref]   # 0.24.6
#   master_version.sh next [ref]     # 0.24.7 (patch + 1)
set -euo pipefail

cmd="${1:-latest}"
ref="${2:-HEAD}"

subject="$(git log --format=%s -E --grep='^[0-9]+\.[0-9]+\.[0-9]+: ' -n 1 "$ref")"
if [[ -z "$subject" ]]; then
  echo "no MAJOR.MINOR.PATCH: subject on ${ref}" >&2
  exit 1
fi
version="${subject%%:*}"

case "$cmd" in
  latest)
    printf '%s\n' "$version"
    ;;
  next)
    IFS=. read -r maj min pat <<<"$version"
    printf '%s.%s.%s\n' "$maj" "$min" "$((pat + 1))"
    ;;
  *)
    echo "usage: master_version.sh latest|next [ref]" >&2
    exit 2
    ;;
esac
