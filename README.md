# gregobot

Personal Telegram bot for fitness and productivity tracking, with an Axum REST admin API.

Tracks body weight, training sessions, nutrition/meals, tasks, reminders, and optional AI coaching (Google Gemini). **Single-tenant / personal deploy** — not a multi-user SaaS.

**Stack:** Tokio · Axum · Teloxide · SQLx · SQLite  
**License:** [MIT OR Apache-2.0](LICENSE)

> Security note: put TLS (or a reverse proxy) in front of any internet-facing deploy. All `/api/*` routes require an `X-API-Key`. See [SECURITY.md](SECURITY.md).

---

## Features

- Telegram commands for weight, training, nutrition, tasks, reminders, and more
- Optional Gemini AI (`/ask`, proactive check-ins, daily recap, nudges, RAG)
- REST admin API for the same domains
- SQLite persistence (including capped app logs and AI traces)
- Docker image + optional registry/Watchtower CI

---

## Quick start (local, polling)

No public URL or ngrok required.

### Prerequisites

- Rust 1.85+ (`rustup update stable`)
- A Telegram bot token from [@BotFather](https://t.me/BotFather)

### Setup

```sh
git clone https://github.com/Wuraim/gregobot-oss.git
cd gregobot-oss
cp .env.example .env
```

Edit `.env`:

```sh
BOT_TOKEN=your_botfather_token
DATABASE_URL=sqlite:data.db
BOT_MODE=polling
PORT=8080
# Optional: ADMIN_API_KEY=a-separate-long-random-secret
# Optional: ADMIN_TELEGRAM_USER_ID=your_telegram_numeric_id
```

```sh
cargo run
```

The server listens on `http://127.0.0.1:8080` (bound to `0.0.0.0` for Docker). Talk to the bot in Telegram; call the REST API with:

```sh
curl -H "X-API-Key: $BOT_TOKEN" http://127.0.0.1:8080/api/users
```

If you set `ADMIN_API_KEY`, use that value instead of `BOT_TOKEN` for `X-API-Key`.

### Webhook mode (production)

```sh
BOT_MODE=webhook
WEBHOOK_URL=https://your.example.com
```

Telegram posts to `{WEBHOOK_URL}/webhook/{BOT_TOKEN}`. Treat that URL as secret — anyone who sees the path has your bot token. Prefer putting the service behind a reverse proxy and a dedicated `ADMIN_API_KEY` for REST.

---

## Environment variables

See `.env.example` and `src/config.rs` for the full list.

| Variable | Required | Description |
|---|---|---|
| `BOT_TOKEN` | yes | From @BotFather |
| `ADMIN_API_KEY` | no | REST `X-API-Key` (defaults to `BOT_TOKEN`) |
| `BOT_MODE` | no | `webhook` (default) or `polling` |
| `WEBHOOK_URL` | webhook mode | Public HTTPS base URL, no trailing slash |
| `DATABASE_URL` | yes | e.g. `sqlite:data.db` |
| `PORT` | no | HTTP port (default `8080`) |
| `ADMIN_TELEGRAM_USER_ID` | no | Seed this user as admin on startup |
| `GEMINI_API_TOKEN` | no | Enables AI features |
| `RAG_ENABLED` | no | Long-term RAG memory (default `false`) |

---

## Bot commands (selected)

| Command | Description |
|---|---|
| `/weight 82.5` | Record weight (kg) |
| `/lastweight` | Latest weight |
| `/weightchart [30\|90\|all]` | Weight history chart |
| `/nutrition [today\|YYYY-MM-DD]` | Daily nutrition table |
| `/training Ran 5km` | Log a training session |
| `/lasttraining` | Last training entries |
| `/help` | Full command list |

AI commands (`/ask`, etc.) need `GEMINI_API_TOKEN`.

---

## Admin REST API

All `/api/*` routes require header `X-API-Key: <ADMIN_API_KEY or BOT_TOKEN>`.  
`/health`, `/logs`, and `/ai/logs` (HTML viewers) stay unauthenticated; the viewers call authenticated JSON APIs from the browser (you paste the key in the UI).

| Method | Path | Description |
|---|---|---|
| `GET` | `/api/weight/:telegram_user_id` | Weight entries |
| `GET` | `/api/weight/:telegram_user_id/latest` | Latest weight |
| `POST` | `/api/weight` | Insert weight `{ telegram_user_id, weight_kg }` |
| `GET` | `/api/training/:telegram_user_id` | Training entries |
| `POST` | `/api/training` | Insert training `{ telegram_user_id, description }` |
| `GET`/`POST` | `/api/users` | List / create users |
| `DELETE` | `/api/users/:telegram_user_id` | Delete user |
| `GET`/`POST`/`PATCH`/`DELETE` | `/api/foods…` | Food catalog |
| `GET`/`POST`/`DELETE` | `/api/meals…` | Meal diary |
| `GET` | `/api/nutrition/:telegram_user_id` | Day rollup |
| `GET` | `/api/logs` | App logs |
| `GET` | `/api/ai/logs` | AI traces |

---

## Docker

```sh
docker build -t gregobot .

docker run -d \
  --name gregobot \
  -p 8080:8080 \
  -v $(pwd)/data:/data \
  -e BOT_TOKEN=your_token \
  -e ADMIN_API_KEY=your_admin_api_key \
  -e BOT_MODE=polling \
  -e DATABASE_URL=sqlite:/data/data.db \
  -e PORT=8080 \
  gregobot
```

Mount a volume for the SQLite file so data (and persisted logs) survive restarts. Do not expose port `8080` on a public IP without TLS and a strong `ADMIN_API_KEY`.

---

## CI (optional)

`.github/workflows/docker.yml` can build and push an image when repository secrets `REGISTRY_*` are set. Watchtower secrets are optional. Omit the workflow secrets if you only want source builds.

Commits that land on `master` use `MAJOR.MINOR.PATCH: description` (see `AGENTS.md`).

---

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). Please report security issues per [SECURITY.md](SECURITY.md).

## License

Dual-licensed under MIT or Apache-2.0. Bundled DejaVu font licensing: [assets/fonts/NOTICE](assets/fonts/NOTICE).
