#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
script="$root/scripts/master_version.sh"

# GitHub Actions runners have no user.name / user.email. Set identity on the
# throwaway repos only — do not touch the caller's global git config.
init_test_repo() {
  local dir="$1"
  git init -q "$dir"
  git -C "$dir" config user.email "master-version-test@example.com"
  git -C "$dir" config user.name "master_version_test"
  git -C "$dir" checkout -q -B master
}

tmp="$(mktemp -d)"
empty=""
cleanup() { rm -rf "$tmp" "${empty:-}"; }
trap cleanup EXIT

init_test_repo "$tmp"
git -C "$tmp" commit --allow-empty -q -m "chore: setup"
git -C "$tmp" commit --allow-empty -q -m "0.1.2: first versioned"
git -C "$tmp" commit --allow-empty -q -m "unversioned follow-up"

(
  cd "$tmp"
  latest="$("$script" latest)"
  next="$("$script" next)"
  [[ "$latest" == "0.1.2" ]] || {
    echo "expected latest 0.1.2, got ${latest}" >&2
    exit 1
  }
  [[ "$next" == "0.1.3" ]] || {
    echo "expected next 0.1.3, got ${next}" >&2
    exit 1
  }
)

empty="$(mktemp -d)"
init_test_repo "$empty"
git -C "$empty" commit --allow-empty -q -m "no versions here"
if (cd "$empty" && "$script" latest >/dev/null 2>&1); then
  echo "repo without x.y.z: subjects must fail" >&2
  exit 1
fi

if "$script" bogus >/dev/null 2>&1; then
  echo "bogus command must fail" >&2
  exit 1
fi

echo "master_version_test: ok"
