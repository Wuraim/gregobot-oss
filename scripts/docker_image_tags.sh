#!/usr/bin/env bash
# Compute Watchtower-friendly image tags for a master commit.
# Required env: REGISTRY_IMAGE, GITHUB_SHA
# Optional env: COMMIT_MESSAGE, GITHUB_OUTPUT
set -euo pipefail

if [[ -z "${REGISTRY_IMAGE:-}" ]]; then
  echo "REGISTRY_IMAGE is not set" >&2
  exit 1
fi
if [[ -z "${GITHUB_SHA:-}" ]]; then
  echo "GITHUB_SHA is not set" >&2
  exit 1
fi

short_sha="${GITHUB_SHA::7}"
created="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
tags="${REGISTRY_IMAGE}:latest"$'\n'"${REGISTRY_IMAGE}:sha-${short_sha}"
version=""
msg="${COMMIT_MESSAGE:-}"
if [[ "${msg}" =~ ^([0-9]+\.[0-9]+\.[0-9]+): ]]; then
  version="${BASH_REMATCH[1]}"
  tags="$tags"$'\n'"${REGISTRY_IMAGE}:$version"
fi
image_version="${version:-$short_sha}"

if [[ -n "${GITHUB_OUTPUT:-}" ]]; then
  {
    echo "version=${version}"
    echo "image_version=${image_version}"
    echo "short_sha=${short_sha}"
    echo "created=${created}"
    echo "tags<<EOF"
    echo "$tags"
    echo "EOF"
  } >> "$GITHUB_OUTPUT"
else
  printf '%s\n' "$tags"
fi
