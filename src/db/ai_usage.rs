use anyhow::Result;
use sqlx::SqlitePool;

use crate::embeddings::EmbedUsage;

/// Purpose recorded on each Gemini call so spend can be split by lever.
pub const KIND_ASK: &str = "ask";
pub const KIND_PROACTIVE: &str = "proactive";
pub const KIND_DAILY_RECAP: &str = "daily_recap";
pub const KIND_NUDGE: &str = "nudge";
pub const KIND_TRAINING_EXTRACT: &str = "training_extract";
pub const KIND_COMPRESS: &str = "compress";
pub const KIND_EMBED_QUERY: &str = "embed_query";
pub const KIND_EMBED_RAG: &str = "embed_rag";
pub const KIND_EMBED_PHOTO: &str = "embed_photo";
pub const KIND_EMBED_EXERCISE: &str = "embed_exercise";
pub const KIND_EMBED_SEARCH_PHOTOS: &str = "embed_search_photos";
pub const KIND_IMAGE: &str = "image";
pub const KIND_IMAGE_GATE: &str = "image_gate";
pub const KIND_UNKNOWN: &str = "unknown";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpendWindow {
    All,
    Days(u32),
}

impl SpendWindow {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "" | "all" => Some(Self::All),
            "7d" | "7" => Some(Self::Days(7)),
            "30d" | "30" => Some(Self::Days(30)),
            _ => None,
        }
    }

    pub fn since_sqlite(self) -> Option<String> {
        match self {
            Self::All => None,
            Self::Days(d) => {
                let ts = chrono::Utc::now() - chrono::Duration::days(i64::from(d));
                Some(ts.format("%Y-%m-%d %H:%M:%S").to_string())
            }
        }
    }

    pub fn label(self) -> String {
        match self {
            Self::All => "all time".into(),
            Self::Days(d) => format!("last {d} days"),
        }
    }
}

/// `/usercost <user> [7d|30d|all]` — window is the last token when present.
pub fn parse_usercost_arg(arg: &str) -> Option<(String, SpendWindow)> {
    let arg = arg.trim();
    if arg.is_empty() {
        return None;
    }
    let mut parts: Vec<&str> = arg.split_whitespace().collect();
    let window = if parts.len() >= 2 {
        let last = parts[parts.len() - 1];
        match last.to_ascii_lowercase().as_str() {
            "7d" | "7" | "30d" | "30" | "all" => {
                let w = SpendWindow::parse(last).unwrap_or(SpendWindow::All);
                parts.pop();
                w
            }
            _ => SpendWindow::All,
        }
    } else {
        SpendWindow::All
    };
    let user = parts.join(" ");
    if user.is_empty() {
        None
    } else {
        Some((user, window))
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct UsageTokens {
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
    pub cached_tokens: i64,
    pub image_tokens: i64,
}

impl From<EmbedUsage> for UsageTokens {
    fn from(usage: EmbedUsage) -> Self {
        Self {
            prompt_tokens: usage.prompt_tokens,
            completion_tokens: 0,
            total_tokens: usage.prompt_tokens,
            cached_tokens: 0,
            image_tokens: usage.image_tokens,
        }
    }
}

pub struct ModelUsageRow {
    pub model: String,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
    pub request_count: i64,
    pub cached_tokens: i64,
    pub image_tokens: i64,
    pub zero_output_requests: i64,
    pub zero_token_requests: i64,
}

pub struct KindUsageRow {
    pub kind: String,
    pub model: String,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
    pub request_count: i64,
    pub cached_tokens: i64,
    pub image_tokens: i64,
    pub zero_output_requests: i64,
    pub zero_token_requests: i64,
}

pub struct UserModelUsageRow {
    pub telegram_user_id: i64,
    pub username: Option<String>,
    pub model: String,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
    pub request_count: i64,
    pub cached_tokens: i64,
    pub image_tokens: i64,
    pub zero_output_requests: i64,
    pub zero_token_requests: i64,
}

#[cfg(test)]
pub async fn insert(
    pool: &SqlitePool,
    user_id: i64,
    model: &str,
    prompt_tokens: i64,
    completion_tokens: i64,
    total_tokens: i64,
) -> Result<()> {
    insert_detailed(
        pool,
        user_id,
        model,
        UsageTokens {
            prompt_tokens,
            completion_tokens,
            total_tokens,
            cached_tokens: 0,
            image_tokens: 0,
        },
        KIND_ASK,
        None,
    )
    .await
}

pub async fn insert_detailed(
    pool: &SqlitePool,
    user_id: i64,
    model: &str,
    tokens: UsageTokens,
    kind: &str,
    request_id: Option<&str>,
) -> Result<()> {
    let prompt_tokens = tokens.prompt_tokens;
    let completion_tokens = tokens.completion_tokens;
    let total_tokens = tokens.total_tokens;
    let cached_tokens = tokens.cached_tokens;
    let image_tokens = tokens.image_tokens;
    let kind = if kind.is_empty() { KIND_UNKNOWN } else { kind };
    sqlx::query!(
        "INSERT INTO ai_usage (user_id, model, prompt_tokens, completion_tokens, total_tokens, cached_tokens, image_tokens, kind, request_id) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        user_id,
        model,
        prompt_tokens,
        completion_tokens,
        total_tokens,
        cached_tokens,
        image_tokens,
        kind,
        request_id,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Drop `ai_usage` rows older than `days`. Returns how many rows were deleted.
pub async fn prune_older_than_days(pool: &SqlitePool, days: i64) -> Result<u64> {
    let modifier = format!("-{days} days");
    let res = sqlx::query("DELETE FROM ai_usage WHERE created_at < datetime('now', ?)")
        .bind(modifier)
        .execute(pool)
        .await?;
    Ok(res.rows_affected())
}

pub async fn log_best_effort_detailed(
    pool: &SqlitePool,
    user_id: i64,
    model: &str,
    tokens: UsageTokens,
    kind: &str,
    request_id: Option<&str>,
) {
    if crate::db::sqlite_full::is_open() {
        return;
    }
    if let Err(e) = insert_detailed(pool, user_id, model, tokens, kind, request_id).await {
        if crate::db::sqlite_full::silence_if_full(pool, &e).await {
            return;
        }
        tracing::warn!(user_id = %user_id, model, kind, "Failed to log AI usage: {e}");
    }
}

pub async fn log_embed_usage(
    pool: &SqlitePool,
    user_id: i64,
    model: &str,
    usage: EmbedUsage,
    kind: &str,
    request_id: Option<&str>,
) {
    log_best_effort_detailed(pool, user_id, model, usage.into(), kind, request_id).await;
}

fn usage_since_filter(window: SpendWindow) -> String {
    window
        .since_sqlite()
        .unwrap_or_else(|| "0000-01-01 00:00:00".to_string())
}

pub async fn by_model_for_user(
    pool: &SqlitePool,
    user_id: i64,
    window: SpendWindow,
) -> Result<Vec<ModelUsageRow>> {
    let since = usage_since_filter(window);
    let rows = sqlx::query!(
        r#"
        SELECT model AS "model!",
               COALESCE(SUM(prompt_tokens), 0) AS "prompt_tokens!: i64",
               COALESCE(SUM(completion_tokens), 0) AS "completion_tokens!: i64",
               COALESCE(SUM(total_tokens), 0) AS "total_tokens!: i64",
               COUNT(*) AS "request_count!: i64",
               COALESCE(SUM(cached_tokens), 0) AS "cached_tokens!: i64",
               COALESCE(SUM(image_tokens), 0) AS "image_tokens!: i64",
               COALESCE(SUM(CASE WHEN completion_tokens <= 0 THEN 1 ELSE 0 END), 0) AS "zero_output_requests!: i64",
               COALESCE(SUM(CASE WHEN prompt_tokens <= 0 AND completion_tokens <= 0 THEN 1 ELSE 0 END), 0) AS "zero_token_requests!: i64"
        FROM ai_usage
        WHERE user_id = ? AND created_at >= ?
        GROUP BY model
        "#,
        user_id,
        since,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| ModelUsageRow {
            model: r.model,
            prompt_tokens: r.prompt_tokens,
            completion_tokens: r.completion_tokens,
            total_tokens: r.total_tokens,
            request_count: r.request_count,
            cached_tokens: r.cached_tokens,
            image_tokens: r.image_tokens,
            zero_output_requests: r.zero_output_requests,
            zero_token_requests: r.zero_token_requests,
        })
        .collect())
}

pub async fn by_kind_for_user(
    pool: &SqlitePool,
    user_id: i64,
    window: SpendWindow,
) -> Result<Vec<KindUsageRow>> {
    let since = usage_since_filter(window);
    let rows = sqlx::query!(
        r#"
        SELECT kind AS "kind!",
               model AS "model!",
               COALESCE(SUM(prompt_tokens), 0) AS "prompt_tokens!: i64",
               COALESCE(SUM(completion_tokens), 0) AS "completion_tokens!: i64",
               COALESCE(SUM(total_tokens), 0) AS "total_tokens!: i64",
               COUNT(*) AS "request_count!: i64",
               COALESCE(SUM(cached_tokens), 0) AS "cached_tokens!: i64",
               COALESCE(SUM(image_tokens), 0) AS "image_tokens!: i64",
               COALESCE(SUM(CASE WHEN completion_tokens <= 0 THEN 1 ELSE 0 END), 0) AS "zero_output_requests!: i64",
               COALESCE(SUM(CASE WHEN prompt_tokens <= 0 AND completion_tokens <= 0 THEN 1 ELSE 0 END), 0) AS "zero_token_requests!: i64"
        FROM ai_usage
        WHERE user_id = ? AND created_at >= ?
        GROUP BY kind, model
        "#,
        user_id,
        since,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| KindUsageRow {
            kind: r.kind,
            model: r.model,
            prompt_tokens: r.prompt_tokens,
            completion_tokens: r.completion_tokens,
            total_tokens: r.total_tokens,
            request_count: r.request_count,
            cached_tokens: r.cached_tokens,
            image_tokens: r.image_tokens,
            zero_output_requests: r.zero_output_requests,
            zero_token_requests: r.zero_token_requests,
        })
        .collect())
}

pub async fn all_by_user_and_model(pool: &SqlitePool) -> Result<Vec<UserModelUsageRow>> {
    let rows = sqlx::query!(
        r#"
        SELECT u.telegram_user_id AS telegram_user_id,
               u.username AS username,
               a.model AS "model!",
               COALESCE(SUM(a.prompt_tokens), 0) AS "prompt_tokens!: i64",
               COALESCE(SUM(a.completion_tokens), 0) AS "completion_tokens!: i64",
               COALESCE(SUM(a.total_tokens), 0) AS "total_tokens!: i64",
               COUNT(*) AS "request_count!: i64",
               COALESCE(SUM(a.cached_tokens), 0) AS "cached_tokens!: i64",
               COALESCE(SUM(a.image_tokens), 0) AS "image_tokens!: i64",
               COALESCE(SUM(CASE WHEN a.completion_tokens <= 0 THEN 1 ELSE 0 END), 0) AS "zero_output_requests!: i64",
               COALESCE(SUM(CASE WHEN a.prompt_tokens <= 0 AND a.completion_tokens <= 0 THEN 1 ELSE 0 END), 0) AS "zero_token_requests!: i64"
        FROM ai_usage a
        JOIN users u ON u.id = a.user_id
        GROUP BY u.id, a.model
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| UserModelUsageRow {
            telegram_user_id: r.telegram_user_id,
            username: r.username,
            model: r.model,
            prompt_tokens: r.prompt_tokens,
            completion_tokens: r.completion_tokens,
            total_tokens: r.total_tokens,
            request_count: r.request_count,
            cached_tokens: r.cached_tokens,
            image_tokens: r.image_tokens,
            zero_output_requests: r.zero_output_requests,
            zero_token_requests: r.zero_token_requests,
        })
        .collect())
}

/// How many `ai_usage` rows for this user+model on the current UTC calendar day.
pub async fn count_today_for_model(pool: &SqlitePool, user_id: i64, model: &str) -> Result<i64> {
    let n: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM ai_usage WHERE user_id = ? AND model = ? AND date(created_at) = date('now')",
    )
    .bind(user_id)
    .bind(model)
    .fetch_one(pool)
    .await?;
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::pricing;
    use crate::db::tests::test_db;
    use crate::db::users::{Role, insert as insert_user};

    #[test]
    fn parse_usercost_arg_splits_window() {
        let (user, w) = parse_usercost_arg("@alice 7d").unwrap();
        assert_eq!(user, "@alice");
        assert_eq!(w, SpendWindow::Days(7));
        let (user, w) = parse_usercost_arg("12345").unwrap();
        assert_eq!(user, "12345");
        assert_eq!(w, SpendWindow::All);
        assert!(parse_usercost_arg("").is_none());
    }

    #[tokio::test]
    async fn ranks_users_by_estimated_spend_not_tokens() {
        let (pool, path) = test_db().await;
        let lite = insert_user(&pool, 101, Role::Sportive, Some("lite".into()), None, None)
            .await
            .unwrap();
        let flash = insert_user(&pool, 202, Role::Sportive, Some("flash".into()), None, None)
            .await
            .unwrap();
        insert(
            &pool,
            lite.id,
            "gemini-2.5-flash-lite",
            10_000_000,
            0,
            10_000_000,
        )
        .await
        .unwrap();
        insert(
            &pool,
            flash.id,
            &format!("gemini-{}.{}-flash", 3, 5),
            1_000_000,
            100_000,
            1_100_000,
        )
        .await
        .unwrap();
        insert(&pool, flash.id, "gemini-2.5-flash-image", 0, 0, 0)
            .await
            .unwrap();

        let ranked = pricing::rank_users(&all_by_user_and_model(&pool).await.unwrap(), 10);
        assert_eq!(ranked[0].telegram_user_id, 202);
        assert_eq!(ranked[1].telegram_user_id, 101);
        assert!(ranked[0].spend.usd > ranked[1].spend.usd);
        // flash chat $2.40 + one $0.039 image
        assert!((ranked[0].spend.usd - 2.439).abs() < 1e-9);
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn by_model_for_user_splits_models() {
        let (pool, path) = test_db().await;
        let user = insert_user(&pool, 303, Role::Sportive, Some("u".into()), None, None)
            .await
            .unwrap();
        insert(&pool, user.id, "gemini-2.5-flash", 100, 20, 120)
            .await
            .unwrap();
        insert(&pool, user.id, "gemini-2.5-flash", 50, 10, 60)
            .await
            .unwrap();
        insert(
            &pool,
            user.id,
            &format!("gemini-embedding-{}", 2),
            40,
            0,
            40,
        )
        .await
        .unwrap();
        let rows = by_model_for_user(&pool, user.id, SpendWindow::All)
            .await
            .unwrap();
        let summary = pricing::summarize(&rows);
        assert_eq!(summary.request_count, 3);
        assert_eq!(summary.prompt_tokens, 190);
        assert_eq!(summary.by_model.len(), 2);
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn cached_and_image_tokens_round_trip_into_spend() {
        let (pool, path) = test_db().await;
        let user = insert_user(&pool, 404, Role::Sportive, Some("c".into()), None, None)
            .await
            .unwrap();
        insert_detailed(
            &pool,
            user.id,
            "gemini-2.5-flash",
            UsageTokens {
                prompt_tokens: 1_000_000,
                completion_tokens: 0,
                total_tokens: 1_000_000,
                cached_tokens: 1_000_000,
                image_tokens: 0,
            },
            KIND_ASK,
            Some("req-cache"),
        )
        .await
        .unwrap();
        insert_detailed(
            &pool,
            user.id,
            &format!("gemini-embedding-{}", 2),
            UsageTokens {
                prompt_tokens: 1_000_000,
                completion_tokens: 0,
                total_tokens: 1_000_000,
                cached_tokens: 0,
                image_tokens: 200_000,
            },
            KIND_EMBED_QUERY,
            Some("req-cache"),
        )
        .await
        .unwrap();
        let rows = by_model_for_user(&pool, user.id, SpendWindow::All)
            .await
            .unwrap();
        let flash = rows.iter().find(|r| r.model.contains("flash")).unwrap();
        let embed = rows.iter().find(|r| r.model.contains("embedding")).unwrap();
        assert_eq!(flash.cached_tokens, 1_000_000);
        assert_eq!(embed.image_tokens, 200_000);
        let summary = pricing::summarize(&rows);
        // cache: 1M × $0.03; embed: 800k × $0.20 + 200k × $0.45 = $0.16 + $0.09
        assert!((summary.usd - 0.28).abs() < 1e-9);
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn by_kind_splits_purpose_and_window_filters() {
        let (pool, path) = test_db().await;
        let user = insert_user(&pool, 505, Role::Sportive, Some("k".into()), None, None)
            .await
            .unwrap();
        insert_detailed(
            &pool,
            user.id,
            "gemini-2.5-flash",
            UsageTokens {
                prompt_tokens: 1_000_000,
                completion_tokens: 0,
                total_tokens: 1_000_000,
                cached_tokens: 0,
                image_tokens: 0,
            },
            KIND_ASK,
            Some("r1"),
        )
        .await
        .unwrap();
        insert_detailed(
            &pool,
            user.id,
            "gemini-2.5-flash-lite",
            UsageTokens {
                prompt_tokens: 1_000_000,
                completion_tokens: 0,
                total_tokens: 1_000_000,
                cached_tokens: 0,
                image_tokens: 0,
            },
            KIND_COMPRESS,
            Some("r1"),
        )
        .await
        .unwrap();
        let kinds = by_kind_for_user(&pool, user.id, SpendWindow::All)
            .await
            .unwrap();
        let spend = crate::ai::pricing::summarize_kinds(&kinds);
        assert_eq!(spend.len(), 2);
        assert_eq!(spend[0].kind, KIND_ASK);
        assert!((spend[0].usd - 0.30).abs() < 1e-9);
        assert_eq!(spend[1].kind, KIND_COMPRESS);
        assert!((spend[1].usd - 0.10).abs() < 1e-9);

        sqlx::query(
            "UPDATE ai_usage SET created_at = datetime('now', '-40 days') WHERE user_id = ?",
        )
        .bind(user.id)
        .execute(&pool)
        .await
        .unwrap();
        let recent = by_model_for_user(&pool, user.id, SpendWindow::Days(30))
            .await
            .unwrap();
        assert!(recent.is_empty());
        let all = by_model_for_user(&pool, user.id, SpendWindow::All)
            .await
            .unwrap();
        assert_eq!(all.len(), 2);
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn prune_older_than_days_keeps_recent_rows() {
        let (pool, path) = test_db().await;
        let user = insert_user(&pool, 303, Role::Sportive, Some("prune".into()), None, None)
            .await
            .unwrap();
        insert(&pool, user.id, "old", 1, 1, 2).await.unwrap();
        insert(&pool, user.id, "new", 1, 1, 2).await.unwrap();
        sqlx::query(
            "UPDATE ai_usage SET created_at = datetime('now', '-120 days') WHERE model = 'old'",
        )
        .execute(&pool)
        .await
        .unwrap();
        let deleted = prune_older_than_days(&pool, 90).await.unwrap();
        assert_eq!(deleted, 1);
        let models: Vec<String> = sqlx::query_scalar("SELECT model FROM ai_usage ORDER BY model")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(models, vec!["new".to_string()]);
        let _ = std::fs::remove_file(&path);
    }
}
