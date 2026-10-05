---
name: cargo-deps
description: Weekly 16:00 Europe/Paris update of gregobot Cargo crates. Write or lock unit tests first, run them, bump one package, adapt the code, then re-run the same tests. Re-subscribe the timer if it expired.
---

# Weekly Cargo dependency update

Standing duty: every **Wednesday at 16:00 Europe/Paris**, update **direct**
Cargo packages (and adapt call sites), proving behavior with unit tests
**before** and **after** each bump.

This is separate from the daily ERROR/WARN review
(`.cursor/skills/prod-log-review/SKILL.md`) and the Mon/Thu usage review
(`.cursor/skills/usage-review/SKILL.md`). Do not mix a crate bump into those
timers.

## Schedule

| Season | Paris | UTC cron |
|---|---|---|
| CEST (last Sunday of March → last Sunday of October) | 16:00 | `0 14 * * 3` |
| CET | 16:00 | `0 15 * * 3` |

Timer name: `weekly-cargo-deps-update` (`cursor-subscriptions` `subscribe_timer`).
Timers expire and dedupe by name. At the start of every run:

1. `list_subscriptions`.
2. If `weekly-cargo-deps-update` is missing or `expiresAt` is near, `unsubscribe`
   (if present) then `subscribe_timer` again with the cron for the current
   season and the prompt below.

Do not create a second timer with a different name.

## Prompt to store on the timer

```
Weekly Wednesday 16:00 Europe/Paris gregobot Cargo dependency update.

Follow .cursor/skills/cargo-deps/SKILL.md.

1. Fetch origin/master. Work on this conversation's cursor/* cargo-deps branch (create from origin/master if needed).
2. List direct deps with scripts/cargo_direct_deps.sh. For each crate, compare locked version to crates.io max_stable_version (script --latest, or curl crates.io). Skip yanked and pre-releases.
3. For EACH crate that has a newer stable release: (a) write or extend unit tests that lock today's behavior of OUR wrappers/call sites if coverage is thin; (b) cargo test must pass on the current version; (c) bump Cargo.toml if the req does not allow the new version, then cargo update -p <crate>; (d) adapt the codebase to compile and keep the same behavior; (e) cargo test again — same tests must pass. Never update first and write tests after.
4. One crate (or one coupled family: sqlx+libsqlite3-sys+sqlite-vec, plotters+png+image) at a time. Do not cargo update the whole lockfile until directs are done.
5. After directs: optional cargo update for remaining compatible transitives, with a full cargo test before and after. sqlx bumps: regenerate .sqlx/ (DATABASE_URL=sqlite:data.db cargo sqlx prepare) when query macros break.
6. cargo test, cargo clippy, cargo fmt -- --check. Commit, push, update or open a PR titled x.y.z: … (`scripts/master_version.sh next origin/master`). Immediately before merge, fetch origin/master and retitle if next moved — never reuse an x.y.z already on master. If the PR has one commit, that git subject must equal the PR title (repo squash uses COMMIT_OR_PR_TITLE). Split a major of axum/teloxide/sqlx into its own PR.
7. Re-subscribe weekly-cargo-deps-update if expired. CEST cron 0 14 * * 3; CET cron 0 15 * * 3.
```

## How to update (mandatory test sandwich)

**Never bump a crate before the tests that pin our behavior exist and pass.**

For **each** outdated direct crate (or one coupled family — see below):

1. **Find call sites** in `src/` (and `scripts/` if the crate is used there).
   Note what our code promises: parse results, HTTP status mapping, SQL rows,
   chart bytes, keyboard markup, error strings we show users.
2. **Write tests first** when existing `#[cfg(test)]` modules do not already
   lock that behavior. Tests assert **gregobot** behavior, not the crate's
   private internals. Prefer table-driven unit tests next to the wrapper
   (`src/datetime.rs` for `chrono` / `chrono-tz` is the pattern).
   `#[cfg(test)]` helpers are fine; do not add `#[allow(dead_code)]`.
3. **Baseline:** `cargo test` (at least the new/targeted tests, then the
   full suite if the targeted run is green). **Stop** if this fails — fix
   tests or code on the current crate versions. Do not touch `Cargo.toml` /
   `Cargo.lock` yet.
4. **Update**
   - Allowed by the current req: `cargo update -p <crate>`
   - Not allowed: bump the version in `Cargo.toml`, then `cargo update -p <crate>`
   - Prefer `max_stable_version` on crates.io. Skip yanked crates and
     pre-releases (`-alpha` / `-beta` / `-rc`) unless we are already on that
     channel.
5. **Adapt** the codebase: fix compile errors, clippy, renamed APIs, changed
   defaults. Restore previous **product** behavior. If sqlx query macros fail,
   apply migrations if needed and regenerate `.sqlx/`:
   `DATABASE_URL=sqlite:data.db cargo sqlx prepare`
6. **Re-test:** run the **same** tests as step 3, then full `cargo test`.
   They must pass. If they fail, fix **our** code to match the tests. Change
   tests only when the new crate behavior is an intentional, documented
   product change (mention it in the PR body). If you cannot restore
   behavior, **revert that crate** (`git checkout -- Cargo.toml Cargo.lock`
   plus any failed adapt) and report it — do not land a broken bump.

Then move to the next crate. Repeat the whole sandwich; do not queue five
bumps behind one baseline run.

### Families (treat as one sandwich)

| Family | Why together |
|---|---|
| `sqlx` + `libsqlite3-sys` + `sqlite-vec` | SQLite ABI + `vec0` auto-extension |
| `plotters` + `png` + `image` | Chart/PNG encoding |
| `serde` + `serde_json` | Serialization |
| `tracing` + `tracing-subscriber` | Logging |

`tokio`, `axum`, `teloxide`, `reqwest` stay **one crate at a time** (majors
especially). A major of `axum` / `teloxide` / `sqlx` gets its **own PR**.

### Transitive-only lockfile refresh

After every direct crate is handled (updated or skipped):

1. Full `cargo test` (baseline).
2. `cargo update` (compatible versions still inside existing reqs).
3. Full `cargo test` again.

Do not do this first, and do not use it to sneak in a direct major.

## What to skip vs land

**Land on this timer**

- Compatible upgrades (patch/minor inside the req, or a req bump that
  compiles and keeps tests green).
- Code adaptations required by the new API.
- Characterization tests written in step 2.

**Skip (report in the reply)**

- Yanked crate versions.
- Pre-release-only newest versions.
- A bump that fails the sandwich after a genuine restore attempt.
- Toolchain/`rustc` upgrades (not this skill).

**Do not**

- Update then “see what broke” before tests exist.
- `cargo update` the entire graph while a major is in flight.
- Leave `Cargo.lock` dirty without a passing `cargo test`.
- Print tokens, `.env`, or crates.io auth.

## Coverage hints (where to look first)

| Crate | Behavior to pin |
|---|---|
| `chrono` / `chrono-tz` | `src/datetime.rs` parsers and local-day bounds (DST) |
| `teloxide` | command parsing (`src/bot/commands.rs`), markdown (`src/bot/markdown.rs`) |
| `axum` | REST router tests in `src/api/mod.rs` |
| `sqlx` / sqlite family | `src/db/mod.rs` + domain CRUD tests |
| `plotters` / `png` / `image` | graph/table PNG tests under `src/bot/` |
| `serde` / `serde_json` | API payloads and Gemini JSON shapes we parse |
| `uuid` | id format we persist/log |
| `anyhow` | error mapping we expose (keep messages stable if tests assert them) |

Network crates (`reqwest`, Gemini client code) still need a baseline full
`cargo test` plus unit tests around **our** request/response mapping. Do not
call live Telegram or Gemini from this timer.

## Listing versions

```sh
scripts/cargo_direct_deps.sh           # name, req, locked (offline)
scripts/cargo_direct_deps.sh --latest  # plus crates.io max_stable_version
scripts/cargo_direct_deps_test.sh      # helper self-check (offline)
```

`--latest` needs network (crates.io). The default path is offline.

## Git / PR

Stay on this conversation's `cursor/*` branch. Feature-branch commits may be
free-form unless the PR has a single commit (then the git subject must equal
the versioned PR title). The **PR title** must be `MAJOR.MINOR.PATCH: …` —
`scripts/master_version.sh next origin/master` (or minor if a direct major landed).
Re-run that **immediately before merge** if `origin/master` moved; do not reuse an
`x.y.z` already on master.

`cargo test`, `cargo clippy`, and `cargo fmt -- --check` before push. This
work needs crates.io for `cargo update`; `cargo test` itself stays offline
after the crates are fetched.
