# AGENTS.md

Contributor / agent notes for gregobot.

## Project

Rust binary: personal Telegram bot with body-weight / training / nutrition tracking and a REST admin API.
Stack: **Tokio + Axum + Teloxide + SQLx + SQLite**.

## Toolchain

- Rust **edition 2024** — requires Rust 1.85+. Run `rustup update stable` if builds fail.

## Commands

```sh
cargo build          # debug build
cargo build --release
cargo run            # requires .env or exported env vars
cargo test
cargo check          # fast type check
cargo clippy         # lint
cargo fmt            # format
cargo fmt -- --check # format check (CI-style)
```

- `cargo build`, `cargo test`, `cargo clippy`, and `cargo fmt -- --check` run fully offline —
  no `BOT_TOKEN`, `GEMINI_API_TOKEN`, or network needed.
- Tests spin up their own temp SQLite files and run migrations (`src/db/mod.rs::tests::test_db`);
  no `DATABASE_URL` required.
- Running the binary requires a **real** `BOT_TOKEN`. At startup `bot::build` (`src/bot/mod.rs`)
  calls Telegram `get_me()` and, after 3 failed attempts, the process exits with
  `Api(InvalidToken)` — the merged Axum REST API is served only *after* that succeeds.
  For local dev use `BOT_MODE=polling` with a real token. The REST layer itself
  (`api::router()` + `db::init`) can still be exercised in-process via `#[tokio::test]`.
- AI features (`/ask`, proactive, recap, nudges, RAG, image gen) are gated behind
  `GEMINI_API_TOKEN`; leave it unset unless testing those flows.

## First-time setup

```sh
cp .env.example .env           # edit: set BOT_TOKEN, DATABASE_URL, PORT; optional WEBHOOK_URL
cargo build
```

SQLx uses **offline mode** — the `.sqlx/` directory contains pre-computed query metadata so
`sqlx::query!` macros validate without a live database. No `DATABASE_URL` needed at build time.
Migrations run automatically on startup via `sqlx::migrate!()`.

## Environment variables

See `src/config.rs` for the complete typed list. Key ones:

| Variable | Required | Description |
|---|---|---|
| `BOT_TOKEN` | yes | From @BotFather |
| `ADMIN_API_KEY` | no | REST `X-API-Key` (defaults to `BOT_TOKEN`) |
| `BOT_MODE` | no | `webhook` (default) or `polling` for local dev |
| `DATABASE_URL` | yes | `sqlite:data.db` or absolute path |
| `GEMINI_API_TOKEN` | no | Enables AI features |
| `GEMINI_MODEL` | no | Main chat model (default `gemini-2.5-flash`) |
| `GEMINI_SUMMARIZE_MODEL` | no | Cheap summarization model (default `gemini-2.5-flash-lite`) |
| `GEMINI_EMBEDDING_MODEL` | no | Embedding model for RAG (default `gemini-embedding-2`, 3072-dim) |
| `GEMINI_IMAGE_MODEL` | no | Coach image generate/edit (default `gemini-2.5-flash-image`) |
| `GEMINI_IMAGE_MAX_PER_USER_DAY` | no | Max coach images per user per UTC day (default `2`) |
| `RAG_ENABLED` | no | Enable RAG long-term memory (default `false`) |
| `RAG_TOP_K` | no | Retrieved chunks per query (default `5`) |
| `ERROR_CURSOR_WEBHOOK_URL` | no | Optional Automations webhook URL |
| `ERROR_CURSOR_WEBHOOK_BEARER` | no | Optional Automations Bearer token |
| `LOG_MAX_ENTRIES` | no | Max `/logs` lines kept in SQLite across restarts (default `5000`) |
| `AI_LOG_MAX_ENTRIES` | no | Max `/ai/logs` traces kept in SQLite across restarts (default `200`) |

Telegram webhook: `{WEBHOOK_URL}/webhook/{BOT_TOKEN}`

### Local testing (polling mode)

```sh
export DATABASE_URL=sqlite:test.db
export BOT_MODE=polling
cargo run
```

REST API on `http://localhost:8080/api/...` (all `/api/*` routes require `X-API-Key`).

## Architecture note

A single Axum server serves both the Telegram webhook endpoint and the admin REST API.
`webhooks::axum_to_router` returns a `Router` merged with `api::router(state)`.
Both share the same `SqlitePool` via Axum state.

## Adding a new data domain

1. Add the DB module `src/db/<domain>.rs` with CRUD operations.
2. Add a tool definition + execution branch in `src/ai/tools.rs` (see existing tools for the pattern).
   `embed_text` is English-only — do not list FR/EN synonym pairs; embeddings handle cross-language matching.
3. Add translations in `src/i18n.rs` if the domain appears in user-facing text.

## Adding a new table / migration

When adding a migration that **creates a new table** and writing `sqlx::query!` macros referencing it,
the `.sqlx/` offline cache must be regenerated:

```sh
DATABASE_URL=sqlite:data.db sqlx migrate run   # apply migration
DATABASE_URL=sqlite:data.db cargo sqlx prepare # regenerate .sqlx/ query cache
cargo check                                     # now validates against the offline cache
```

Prerequisite: `sqlx-cli` (`cargo install sqlx-cli`).

## Commit / merge to `master`

Commits that **land on `master`** (squash or merge) must use:

```
MAJOR.MINOR.PATCH: Short description
```

Example: `0.24.5: Harden Gemini tool loop`.

- Next version: `scripts/master_version.sh next origin/master`.
- **Squash-merge only** into `master`.
- Feature-branch commits may stay free-form **unless the PR has a single commit** — then the git subject and PR title must both be `x.y.z: …` and must match.
- `.github/workflows/commit-message.yml` enforces this on PRs and `master` pushes.

## Notes

- Bot commands: `src/bot/commands.rs`. REST endpoints: `src/api/mod.rs`.
- All `/api/*` routes require `X-API-Key` (`ADMIN_API_KEY`, or `BOT_TOKEN` when unset).
- `Cargo.lock` is intentionally committed (binary crate convention).
- SQLx offline mode (`.sqlx/` cache) is configured; no `DATABASE_URL` needed at build time.
- Never use `#[allow(dead_code)]`. Test-only helpers use `#[cfg(test)]`; otherwise use the item or delete it.
- `GEMINI_API_TOKEN` enables AI features. RAG is opt-in via `RAG_ENABLED=true`.
- Nudges are enabled by default; use `/nudges [on|off|at N]` to configure.
- Logs persist in SQLite (`app_logs` / `ai_traces`). Viewers at `/logs` and `/ai/logs`; JSON under `/api/logs` and `/api/ai/logs` (API key required).
