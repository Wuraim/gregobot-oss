#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
script="$root/scripts/docker_image_tags.sh"

out="$(REGISTRY_IMAGE=registry.example.com/gregobot GITHUB_SHA=abcdef1234567890 COMMIT_MESSAGE="fix logs" "$script")"
grep -qx "registry.example.com/gregobot:latest" <<<"$out"
grep -qx "registry.example.com/gregobot:sha-abcdef1" <<<"$out"
if grep -q ":1.2.3$" <<<"$out"; then
  echo "plain commit must not add a semver tag" >&2
  exit 1
fi

out="$(REGISTRY_IMAGE=registry.example.com/gregobot GITHUB_SHA=abcdef1234567890 COMMIT_MESSAGE="1.2.3: release" "$script")"
grep -qx "registry.example.com/gregobot:latest" <<<"$out"
grep -qx "registry.example.com/gregobot:1.2.3" <<<"$out"

tmp="$(mktemp)"
REGISTRY_IMAGE=registry.example.com/gregobot GITHUB_SHA=deadbeefcafebabe \
  COMMIT_MESSAGE="0.24.5: watchtower" GITHUB_OUTPUT="$tmp" "$script"
grep -q "image_version=0.24.5" "$tmp"
grep -q "short_sha=deadbee" "$tmp"
grep -q "gregobot:latest" "$tmp"
rm -f "$tmp"

if REGISTRY_IMAGE= GITHUB_SHA=abc "$script" 2>/dev/null; then
  echo "empty REGISTRY_IMAGE must fail" >&2
  exit 1
fi

echo "docker_image_tags_test: ok"
