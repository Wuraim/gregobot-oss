#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
script="$root/scripts/cargo_direct_deps.sh"

out="$("$script")"
if [[ -z "$out" ]]; then
  echo "expected at least one direct dependency" >&2
  exit 1
fi

while IFS=$'\t' read -r name req locked; do
  if [[ -z "$name" || -z "$req" || -z "$locked" ]]; then
    echo "expected 3 TSV columns, got: ${name:-}|${req:-}|${locked:-}" >&2
    exit 1
  fi
  if [[ "$name" == *$'\t'* ]]; then
    echo "name must not contain tabs: $name" >&2
    exit 1
  fi
done <<<"$out"

grep -q $'^axum\t' <<<"$out"
grep -q $'^teloxide\t' <<<"$out"
grep -q $'^sqlx\t' <<<"$out"
grep -q $'^chrono\t' <<<"$out"

teloxide_line="$(grep $'^teloxide\t' <<<"$out")"
IFS=$'\t' read -r _ req locked <<<"$teloxide_line"
if [[ "$req" != ^0.17* && "$req" != 0.17* ]]; then
  echo "expected teloxide req ^0.17, got: $req" >&2
  exit 1
fi
if [[ ! "$locked" =~ ^0\.17\. ]]; then
  echo "expected teloxide locked 0.17.x, got: $locked" >&2
  exit 1
fi

if "$script" --bogus >/dev/null 2>&1; then
  echo "--bogus must fail" >&2
  exit 1
fi

help_out="$("$script" --help 2>&1)" || true
grep -q "usage:" <<<"$help_out"

echo "cargo_direct_deps_test: ok"
