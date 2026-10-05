#!/usr/bin/env bash
# List gregobot's direct Cargo dependencies: name, Cargo.toml req, locked version.
# Usage: cargo_direct_deps.sh [--latest]
#        --latest appends crates.io max_stable_version (needs network).
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

latest=0
for arg in "$@"; do
  case "$arg" in
    --latest) latest=1 ;;
    -h|--help)
      echo "usage: cargo_direct_deps.sh [--latest]" >&2
      exit 0
      ;;
    *)
      echo "usage: cargo_direct_deps.sh [--latest]" >&2
      exit 2
      ;;
  esac
done

export CARGO_DIRECT_DEPS_LATEST="$latest"
python3 - <<'PY'
import json
import os
import subprocess
import sys
import urllib.error
import urllib.request

LATEST = os.environ.get("CARGO_DIRECT_DEPS_LATEST") == "1"
UA = "gregobot-cargo-direct-deps (https://github.com/wuraim/gregobot)"


def cargo_metadata():
    cmd_locked = [
        "cargo",
        "metadata",
        "--format-version",
        "1",
        "--locked",
        "--offline",
    ]
    proc = subprocess.run(cmd_locked, capture_output=True, text=True)
    if proc.returncode != 0:
        proc = subprocess.run(
            ["cargo", "metadata", "--format-version", "1", "--locked"],
            capture_output=True,
            text=True,
        )
    if proc.returncode != 0:
        sys.stderr.write(proc.stderr)
        sys.exit(proc.returncode or 1)
    return json.loads(proc.stdout)


def crates_io_stable(name: str) -> str:
    url = f"https://crates.io/api/v1/crates/{name}"
    req = urllib.request.Request(url, headers={"User-Agent": UA})
    try:
        with urllib.request.urlopen(req, timeout=20) as resp:
            data = json.load(resp)
    except (urllib.error.URLError, TimeoutError, json.JSONDecodeError):
        return "-"
    crate = data.get("crate") or {}
    return crate.get("max_stable_version") or crate.get("max_version") or "-"


def main() -> None:
    meta = cargo_metadata()
    packages = {p["id"]: p for p in meta["packages"]}
    resolve = meta.get("resolve") or {}
    root_id = resolve.get("root")
    if not root_id:
        sys.stderr.write("cargo metadata: missing resolve.root\n")
        sys.exit(1)
    root_pkg = packages[root_id]
    nodes = {n["id"]: n for n in resolve.get("nodes") or []}
    root_node = nodes[root_id]
    locked = {}
    for dep_id in root_node.get("dependencies") or []:
        pkg = packages[dep_id]
        locked[pkg["name"]] = pkg["version"]

    rows = []
    for dep in root_pkg.get("dependencies") or []:
        if dep.get("kind") not in (None, "normal"):
            continue
        if dep.get("path"):
            continue
        name = dep["name"]
        req = dep.get("req") or ""
        lock = locked.get(name, "-")
        rows.append((name, req, lock))
    rows.sort(key=lambda r: r[0])

    latest_by_name = {}
    if LATEST:
        for name, _, _ in rows:
            latest_by_name[name] = crates_io_stable(name)

    for name, req, lock in rows:
        cols = [name, req, lock]
        if LATEST:
            cols.append(latest_by_name[name])
        print("\t".join(cols))


if __name__ == "__main__":
    main()
PY
