#!/usr/bin/env bash
# Enforce how versioned subjects land on master.
#
# GitHub repo squash title is COMMIT_OR_PR_TITLE: a single-commit PR uses the
# git subject (not the PR title). Rebase-merge copies every feature commit onto
# master. This script matches those cases so CI fails before an unversioned
# subject becomes HEAD of master.
#
# Optional --base MAJOR.MINOR.PATCH (latest versioned subject already on
# master): the new subject(s) must be strictly newer. Stops two merges from
# reusing the same x.y.z (Docker :x.y.z would collide).
#
# Usage:
#   check_master_nomenclature.sh pr [--base x.y.z] <title> <commit_count> [head_subject]
#   check_master_nomenclature.sh push [--base x.y.z] <subject> [subject ...]
set -euo pipefail

root="$(cd "$(dirname "$0")" && pwd)"
check="$root/check_commit_message.sh"

first_line() {
  local s="${1-}"
  s="${s%%$'\n'*}"
  s="${s%"${s##*[![:space:]]}"}"
  printf '%s' "$s"
}

subject_version() {
  local s
  s="$(first_line "$1")"
  if [[ "$s" =~ ^([0-9]+\.[0-9]+\.[0-9]+): ]]; then
    printf '%s' "${BASH_REMATCH[1]}"
    return 0
  fi
  return 1
}

# Exit 0 if $1 > $2 (MAJOR.MINOR.PATCH).
version_gt() {
  local amaj amin apat bmaj bmin bpat
  IFS=. read -r amaj amin apat <<<"$1"
  IFS=. read -r bmaj bmin bpat <<<"$2"
  if ((amaj != bmaj)); then
    ((amaj > bmaj))
    return
  fi
  if ((amin != bmin)); then
    ((amin > bmin))
    return
  fi
  ((apat > bpat))
}

require_newer_than_base() {
  local ver="$1"
  if [[ -z "$base" ]]; then
    return 0
  fi
  if ! version_gt "$ver" "$base"; then
    echo "version ${ver} is not newer than master ${base}" >&2
    echo "  fetch origin/master and run: scripts/master_version.sh next origin/master" >&2
    exit 1
  fi
}

# Newest-first git log: versions must strictly decrease toward older commits,
# and HEAD must be newer than --base when set.
require_increasing_versions() {
  local -a versions=()
  local s v
  for s in "$@"; do
    if v="$(subject_version "$s")"; then
      versions+=("$v")
    fi
  done
  if [[ ${#versions[@]} -eq 0 ]]; then
    return 0
  fi
  require_newer_than_base "${versions[0]}"
  local i
  for ((i = 1; i < ${#versions[@]}; i++)); do
    if ! version_gt "${versions[i - 1]}" "${versions[i]}"; then
      echo "versioned subjects must strictly increase toward HEAD" >&2
      echo "  newer: ${versions[i - 1]}  older: ${versions[i]}" >&2
      echo "  do not reuse an x.y.z already on master" >&2
      exit 1
    fi
  done
}

usage() {
  echo "usage: check_master_nomenclature.sh pr [--base x.y.z] <title> <commit_count> [head_subject]" >&2
  echo "       check_master_nomenclature.sh push [--base x.y.z] <subject> [subject ...]" >&2
  exit 2
}

mode="${1:-}"
shift || true

base=""
if [[ "${1:-}" == "--base" ]]; then
  if [[ -z "${2:-}" ]]; then
    echo "--base requires MAJOR.MINOR.PATCH" >&2
    exit 2
  fi
  base="$2"
  if [[ ! "$base" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "--base must be MAJOR.MINOR.PATCH, got: ${base}" >&2
    exit 2
  fi
  shift 2
fi

case "$mode" in
  pr)
    title="${1-}"
    count="${2-}"
    head_subject="${3-}"
    if [[ -z "$title" || -z "$count" ]]; then
      usage
    fi
    if [[ ! "$count" =~ ^[0-9]+$ ]]; then
      echo "commit_count must be an integer, got: ${count}" >&2
      exit 2
    fi
    "$check" "$title"
    if [[ "$count" -eq 1 ]]; then
      if [[ -z "$head_subject" ]]; then
        echo "single-commit PR: missing git subject" >&2
        exit 2
      fi
      "$check" "$head_subject"
      title_line="$(first_line "$title")"
      head_line="$(first_line "$head_subject")"
      if [[ "$title_line" != "$head_line" ]]; then
        echo "single-commit PR: git subject must equal the PR title" >&2
        echo "  GitHub squash COMMIT_OR_PR_TITLE uses the commit subject" >&2
        echo "  PR title: ${title_line}" >&2
        echo "  git subject: ${head_line}" >&2
        exit 1
      fi
    fi
    require_increasing_versions "$title"
    ;;
  push)
    if [[ $# -eq 0 ]]; then
      usage
    fi
    for subject in "$@"; do
      "$check" "$subject"
    done
    require_increasing_versions "$@"
    ;;
  *)
    usage
    ;;
esac
