# Security Policy

## Product assumptions

gregobot is a **personal / single-tenant** Telegram bot. One process, one SQLite database, Telegram roles (`admin` / `sportive` / `pending`). It is **not** designed as a multi-tenant SaaS.

## Deploy hardening

- Require `X-API-Key` on every `/api/*` route. Prefer a dedicated `ADMIN_API_KEY` over reusing `BOT_TOKEN`.
- Put TLS (Caddy, nginx, Traefik, etc.) in front of the process. Do not expose the raw `PORT` on a public interface without a reverse proxy.
- Webhook mode uses `{WEBHOOK_URL}/webhook/{BOT_TOKEN}`. Anyone who learns that path has your bot token — rotate via [@BotFather](https://t.me/BotFather) if it leaks, and update `ADMIN_API_KEY` if you were still falling back to `BOT_TOKEN`.
- HTML pages `/logs` and `/ai/logs` are public shells; the JSON they fetch still needs the API key.
- Keep `.env`, `*.db`, and production hostnames out of git.

## Reporting a vulnerability

Please **do not** open a public GitHub issue for security bugs.

Prefer one of:

1. GitHub **private vulnerability advisory** on this repository (Security → Advisories → New draft advisory), or
2. Email the maintainer via the address on their GitHub profile.

Include steps to reproduce, impact, and (if possible) a suggested fix. You should receive an acknowledgement within a few days.

## Known follow-ups

- Replace the bot-token webhook path segment with a dedicated random `WEBHOOK_SECRET` so URL leaks do not equal bot compromise.
