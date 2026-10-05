#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
check="$root/scripts/check_commit_message.sh"

assert_ok() {
  if ! "$check" "$1"; then
    echo "expected OK: $1" >&2
    exit 1
  fi
}

assert_fail() {
  if "$check" "$1"; then
    echo "expected FAIL: $1" >&2
    exit 1
  fi
}

assert_ok "0.24.5: Harden Gemini tool loop"
assert_ok $'0.24.4: Webhook cursor error\n\nbody'
assert_ok "0.24.8: Weekly cargo dependency maintenance (#34)"
assert_ok "1.0.0: Major rewrite"
assert_fail "Harden Gemini loops against prod ERROR/WARN storms"
assert_fail "0.24.5 Harden without colon"
assert_fail "v0.24.5: prefixed v"
assert_fail "0.24.5:"
assert_fail "0.24: missing patch"
assert_fail ""

echo "check_commit_message_test: ok"
