#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
check="$root/scripts/check_master_nomenclature.sh"

assert_ok() {
  if ! "$check" "$@"; then
    echo "expected OK: $*" >&2
    exit 1
  fi
}

assert_fail() {
  if "$check" "$@" >/dev/null 2>&1; then
    echo "expected FAIL: $*" >&2
    exit 1
  fi
}

assert_ok pr "0.24.9: Enforce versioned master subjects" 2
assert_ok pr "0.24.9: Enforce versioned master subjects" 1 \
  "0.24.9: Enforce versioned master subjects"
assert_ok pr $'0.24.8: Weekly cargo\n\nbody' 1 $'0.24.8: Weekly cargo\n\nCo-authored-by: x'

assert_fail pr "Add weekly Cargo crate updates" 2
assert_fail pr "0.24.8: Weekly cargo" 1 "Add weekly Cargo crate updates"
assert_fail pr "0.24.8: Weekly cargo" 1 "0.24.8: Different description"
assert_fail pr "0.24.8: Weekly cargo" 1
assert_fail pr "0.24.8: Weekly cargo" not-a-number "0.24.8: Weekly cargo"

assert_ok pr --base 0.24.9 "0.24.10: Unique patch" 2
assert_ok pr --base 0.24.9 "0.25.0: Minor bump" 2
assert_fail pr --base 0.24.9 "0.24.9: Duplicate patch" 2
assert_fail pr --base 0.24.9 "0.24.8: Older than master" 2

assert_ok push "0.24.9: One" "0.24.8: Two (#34)"
assert_fail push "0.24.9: One" "Add weekly Cargo crate updates"
assert_fail push "0.24.10: One" "0.24.10: Two"
assert_ok push --base 0.24.9 "0.24.10: One"
assert_fail push --base 0.24.9 "0.24.9: Same again"
assert_fail push

echo "check_master_nomenclature_test: ok"
