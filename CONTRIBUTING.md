# Contributing

Thanks for taking an interest in gregobot.

## Development

- Rust **1.85+** (edition 2024).
- Copy `.env.example` → `.env` for local runs; tests do not need env vars.
- Prefer `BOT_MODE=polling` for local Telegram testing.

```sh
cargo fmt
cargo clippy
cargo test
cargo check
```

SQLx is in offline mode (`.sqlx/` committed). After adding a migration that new `query!` macros reference:

```sh
DATABASE_URL=sqlite:data.db sqlx migrate run
DATABASE_URL=sqlite:data.db cargo sqlx prepare
```

## Pull requests

- Keep changes focused; match existing style.
- Do not commit `.env`, `*.db`, tokens, or personal production hostnames.
- Do not add `#[allow(dead_code)]` — use the item, `#[cfg(test)]`, or delete it.
- Commits that land on `master` must be titled `MAJOR.MINOR.PATCH: Short description` (squash-merge only). Feature branches may use free-form subjects unless the PR has a single commit (then the subject must match the versioned title).

## Security

See [SECURITY.md](SECURITY.md).
