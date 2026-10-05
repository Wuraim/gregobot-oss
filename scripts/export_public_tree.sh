#!/usr/bin/env bash
# Export a sanitized tree for a clean public repo (Option A).
# Usage: scripts/export_public_tree.sh /path/to/public-checkout
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DEST="${1:?destination directory required}"

mkdir -p "$DEST"
rsync -a --delete \
  --exclude '.git/' \
  --exclude 'target/' \
  --exclude '.env' \
  --exclude '.env.local' \
  --exclude '.env.production' \
  --exclude '*.db' \
  --exclude '*.db-shm' \
  --exclude '*.db-wal' \
  --exclude '*.log' \
  --exclude 'notes/' \
  --exclude '.cursor/skills/prod-logs/' \
  --exclude '.cursor/skills/prod-log-review/' \
  --exclude '.cursor/skills/usage-review/' \
  --exclude '.opencode/' \
  --exclude '.cursor/plans/' \
  --exclude 'scripts/publish_public_repo.sh' \
  "$ROOT/" "$DEST/"

# Keep cargo-deps skill; ensure private ops paths are gone
rm -rf \
  "$DEST/.cursor/skills/prod-logs" \
  "$DEST/.cursor/skills/prod-log-review" \
  "$DEST/.cursor/skills/usage-review" \
  "$DEST/.opencode" \
  "$DEST/notes"

# Ensure example env is present even if a broad exclude ever drops it
cp -f "$ROOT/.env.example" "$DEST/.env.example"

echo "Sanitized tree written to $DEST"
