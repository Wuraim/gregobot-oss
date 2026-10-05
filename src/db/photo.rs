use anyhow::{Context, Result};
use chrono::{NaiveDateTime, Utc};
use sqlx::{FromRow, SqlitePool};

use crate::db::Db;
use crate::embeddings::{self, EMBEDDING_DIMENSION};

/// Max L2 distance to treat two photos as the same training screenshot.
pub const TRAINING_DUP_MAX_DISTANCE: f64 = 0.35;
/// Max L2 distance to mention a photo as a pointer in the /ask prompt.
pub const POINTER_MAX_DISTANCE: f64 = 0.65;

/// A tracked photo message.
pub struct PhotoMessage {
    pub id: i64,
    pub sender_telegram_user_id: i64,
    pub caption: Option<String>,
    pub file_id: Option<String>,
    /// Standardized JPEG bytes, if already ingested.
    pub image_jpeg: Option<Vec<u8>>,
    /// Telegram message date (UTC), falling back to ingest time.
    pub taken_at: String,
    pub embedding: Option<String>,
}

impl PhotoMessage {
    /// Telegram `message.date` stored as UTC `YYYY-MM-DD HH:MM:SS`.
    pub fn taken_at_utc(&self) -> Option<chrono::DateTime<Utc>> {
        NaiveDateTime::parse_from_str(&self.taken_at, "%Y-%m-%d %H:%M:%S")
            .ok()
            .map(|naive| naive.and_utc())
    }
}

pub enum PhotoSearchScope {
    /// Photos this user sent in private chats (`chat_id > 0`).
    Private,
    /// Photos this user sent in this chat only.
    Chat(i64),
}

impl PhotoSearchScope {
    pub fn from_chat_id(chat_id: Option<i64>) -> Self {
        match chat_id {
            Some(id) if id < 0 => Self::Chat(id),
            _ => Self::Private,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PhotoHit {
    pub photo_id: i64,
    pub caption: Option<String>,
    pub taken_at: String,
    pub distance: f64,
    pub training_counted: bool,
}

/// Insert or replace a photo message record (metadata only). Returns `rowid`.
pub async fn save_photo_message(
    pool: &SqlitePool,
    chat_id: i64,
    message_id: i32,
    sender_telegram_user_id: i64,
    caption: Option<String>,
    file_id: Option<String>,
    taken_at: Option<&str>,
) -> Result<i64> {
    let id = sqlx::query_scalar!(
        r#"
        INSERT INTO photo_messages (
            chat_id, message_id, sender_telegram_user_id, caption, file_id, taken_at
        )
        VALUES (?, ?, ?, ?, ?, COALESCE(?, datetime('now')))
        ON CONFLICT(chat_id, message_id) DO UPDATE SET
            sender_telegram_user_id = excluded.sender_telegram_user_id,
            caption = excluded.caption,
            file_id = excluded.file_id,
            taken_at = COALESCE(photo_messages.taken_at, excluded.taken_at)
        RETURNING id AS "id!"
        "#,
        chat_id,
        message_id,
        sender_telegram_user_id,
        caption,
        file_id,
        taken_at,
    )
    .fetch_one(pool)
    .await?;
    Ok(id)
}

/// Store (or replace) the standardized JPEG blob for a tracked photo.
pub async fn save_image_jpeg(
    pool: &SqlitePool,
    chat_id: i64,
    message_id: i32,
    jpeg: &[u8],
) -> Result<()> {
    sqlx::query!(
        r#"
        UPDATE photo_messages
        SET image_jpeg = ?
        WHERE chat_id = ? AND message_id = ?
        "#,
        jpeg,
        chat_id,
        message_id,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Look up a photo message by chat_id and message_id.
pub async fn find_photo_message(
    pool: &SqlitePool,
    chat_id: i64,
    message_id: i32,
) -> Result<Option<PhotoMessage>> {
    let row = sqlx::query!(
        r#"
        SELECT
            id AS "id!",
            sender_telegram_user_id,
            caption,
            file_id,
            image_jpeg,
            COALESCE(taken_at, created_at) AS "taken_at!: String",
            embedding
        FROM photo_messages
        WHERE chat_id = ? AND message_id = ?
        "#,
        chat_id,
        message_id,
    )
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| PhotoMessage {
        id: r.id,
        sender_telegram_user_id: r.sender_telegram_user_id,
        caption: r.caption,
        file_id: r.file_id,
        image_jpeg: r.image_jpeg,
        taken_at: r.taken_at,
        embedding: r.embedding,
    }))
}

/// JPEG bytes for a photo owned by this internal user id.
pub async fn jpeg_for_user(
    pool: &SqlitePool,
    photo_id: i64,
    user_id: i64,
) -> Result<Option<Vec<u8>>> {
    let row = sqlx::query!(
        r#"
        SELECT p.image_jpeg
        FROM photo_messages p
        JOIN users u ON u.telegram_user_id = p.sender_telegram_user_id
        WHERE p.id = ? AND u.id = ?
        "#,
        photo_id,
        user_id,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.and_then(|r| r.image_jpeg))
}

/// Persist the embedding JSON and the sqlite-vec row (same rowid as photo_messages).
pub async fn upsert_embedding(pool: &Db, photo_id: i64, embedding: &[f32]) -> Result<()> {
    if embedding.len() != EMBEDDING_DIMENSION {
        anyhow::bail!(
            "photo embedding has {} dims, expected {EMBEDDING_DIMENSION}",
            embedding.len()
        );
    }
    let json = embeddings::serialize_embedding(embedding);

    let mut tx = pool.begin().await?;
    sqlx::query("UPDATE photo_messages SET embedding = ? WHERE id = ?")
        .bind(&json)
        .bind(photo_id)
        .execute(&mut *tx)
        .await
        .context("failed to store photo embedding json")?;

    sqlx::query("DELETE FROM photo_vec WHERE rowid = ?")
        .bind(photo_id)
        .execute(&mut *tx)
        .await
        .ok();

    sqlx::query("INSERT INTO photo_vec (rowid, embedding) VALUES (?, ?)")
        .bind(photo_id)
        .bind(&json)
        .execute(&mut *tx)
        .await
        .context("failed to insert photo_vec row")?;

    tx.commit().await?;
    Ok(())
}

/// Background: embed the stored JPEG with gemini-embedding-2 (image + caption).
pub fn spawn_embed_photo(
    pool: Db,
    api_key: String,
    model: String,
    user_id: i64,
    photo_id: i64,
    jpeg: Vec<u8>,
    caption: Option<String>,
) {
    if api_key.is_empty() {
        return;
    }
    tokio::spawn(async move {
        match embeddings::embed_image(&api_key, &model, &jpeg, caption.as_deref()).await {
            Ok((vec, usage)) => {
                crate::db::ai_usage::log_embed_usage(
                    &pool,
                    user_id,
                    &model,
                    usage,
                    crate::db::ai_usage::KIND_EMBED_PHOTO,
                    None,
                )
                .await;
                if let Err(e) = upsert_embedding(&pool, photo_id, &vec).await {
                    tracing::warn!(photo_id, "Failed to store photo embedding: {e}");
                }
            }
            Err(e) => {
                tracing::warn!(photo_id, "Failed to embed photo: {e}");
            }
        }
    });
}

/// KNN over this user's photos in `scope`.
pub async fn search(
    pool: &Db,
    user_id: i64,
    query_embedding: &[f32],
    scope: PhotoSearchScope,
    top_k: i64,
    exclude_photo_id: Option<i64>,
    counted_only: bool,
) -> Result<Vec<PhotoHit>> {
    let query_json = embeddings::serialize_embedding(query_embedding);
    let top_k = top_k.clamp(1, 20);
    let exclude = exclude_photo_id.unwrap_or(-1);

    let scope_sql = match scope {
        PhotoSearchScope::Private => "AND p.chat_id > 0",
        PhotoSearchScope::Chat(_) => "AND p.chat_id = ?",
    };
    let counted_sql = if counted_only {
        "AND p.training_counted = 1"
    } else {
        ""
    };

    let sql = format!(
        r#"
        SELECT
            p.id AS photo_id,
            p.caption AS caption,
            COALESCE(p.taken_at, p.created_at) AS taken_at,
            v.distance AS distance,
            p.training_counted AS training_counted
        FROM (
            SELECT rowid, distance
            FROM photo_vec
            WHERE embedding MATCH '{query_json}'
            LIMIT ?
        ) AS v
        JOIN photo_messages p ON p.id = v.rowid
        JOIN users u ON u.telegram_user_id = p.sender_telegram_user_id
        WHERE u.id = ?
          AND p.id != ?
          {scope_sql}
          {counted_sql}
        ORDER BY v.distance
        "#
    );

    // MATCH literal is serialize_embedding([f32]); scope/counted fragments are static.
    let mut q = sqlx::query_as::<_, PhotoHitRow>(sqlx::AssertSqlSafe(sql))
        .bind(top_k)
        .bind(user_id)
        .bind(exclude);
    if let PhotoSearchScope::Chat(chat_id) = scope {
        q = q.bind(chat_id);
    }

    let rows = q.fetch_all(pool).await?;
    Ok(rows
        .into_iter()
        .map(|r| PhotoHit {
            photo_id: r.photo_id,
            caption: r.caption,
            taken_at: r.taken_at,
            distance: r.distance,
            training_counted: r.training_counted != 0,
        })
        .collect())
}

/// If this photo looks like one already counted as training, return that hit.
pub async fn similar_counted_training(
    pool: &Db,
    user_id: i64,
    photo: &PhotoMessage,
    scope: PhotoSearchScope,
) -> Result<Option<PhotoHit>> {
    let Some(json) = photo.embedding.as_deref() else {
        return Ok(None);
    };
    let embedding: Vec<f32> = serde_json::from_str(json).unwrap_or_default();
    if embedding.len() != EMBEDDING_DIMENSION {
        return Ok(None);
    }
    let hits = search(pool, user_id, &embedding, scope, 3, Some(photo.id), true).await?;
    Ok(hits
        .into_iter()
        .find(|h| h.distance <= TRAINING_DUP_MAX_DISTANCE))
}

pub fn format_pointer_context(hits: &[PhotoHit]) -> Option<String> {
    let relevant: Vec<&PhotoHit> = hits
        .iter()
        .filter(|h| h.distance <= POINTER_MAX_DISTANCE)
        .take(2)
        .collect();
    if relevant.is_empty() {
        return None;
    }
    let mut out = String::from(
        "Screenshot/log archive matches (metadata only — do not invent what is in the picture; use search_photos to inspect or send). Never ask for a body or physique photo:\n",
    );
    for h in relevant {
        let cap = h.caption.as_deref().unwrap_or("(no caption)");
        out.push_str(&format!(
            "- photo_id={} taken_at={} caption={}\n",
            h.photo_id, h.taken_at, cap
        ));
    }
    Some(out)
}

/// Atomically mark a photo as counted as a training entry.
/// Returns true only for the first caller (i.e. the photo was not counted yet).
pub async fn try_mark_training_counted(
    pool: &SqlitePool,
    chat_id: i64,
    message_id: i32,
) -> Result<bool> {
    let result = sqlx::query!(
        r#"
        UPDATE photo_messages
        SET training_counted = 1
        WHERE chat_id = ? AND message_id = ? AND training_counted = 0
        "#,
        chat_id,
        message_id,
    )
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

#[derive(FromRow)]
struct PhotoHitRow {
    photo_id: i64,
    caption: Option<String>,
    taken_at: String,
    distance: f64,
    training_counted: i64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::tests::test_db;
    use crate::db::users::{Role, insert as insert_user};

    fn unit(i: usize) -> Vec<f32> {
        let mut v = vec![0.0f32; EMBEDDING_DIMENSION];
        v[i] = 1.0;
        v
    }

    #[tokio::test]
    async fn save_and_find_roundtrip_jpeg_and_taken_at() {
        let (pool, path) = test_db().await;
        let id = save_photo_message(
            &pool,
            1,
            10,
            99,
            Some("cap".into()),
            Some("fid".into()),
            Some("2026-08-01 12:00:00"),
        )
        .await
        .unwrap();
        assert!(id > 0);
        let jpeg = vec![0xff, 0xd8, 0xff, 0xd9];
        save_image_jpeg(&pool, 1, 10, &jpeg).await.unwrap();
        let found = find_photo_message(&pool, 1, 10).await.unwrap().unwrap();
        assert_eq!(found.id, id);
        assert_eq!(found.sender_telegram_user_id, 99);
        assert_eq!(found.caption.as_deref(), Some("cap"));
        assert_eq!(found.file_id.as_deref(), Some("fid"));
        assert_eq!(found.image_jpeg.as_deref(), Some(jpeg.as_slice()));
        assert_eq!(found.taken_at, "2026-08-01 12:00:00");
        assert_eq!(
            found.taken_at_utc().map(|d| d.to_rfc3339()),
            Some("2026-08-01T12:00:00+00:00".into())
        );
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn search_returns_own_private_photos_only() {
        let (pool, path) = test_db().await;
        let a = insert_user(&pool, 1001, Role::Sportive, Some("a".into()), None, None)
            .await
            .unwrap();
        let b = insert_user(&pool, 1002, Role::Sportive, Some("b".into()), None, None)
            .await
            .unwrap();

        let id_a = save_photo_message(
            &pool,
            1001,
            1,
            1001,
            Some("run".into()),
            None,
            Some("2026-08-01 08:00:00"),
        )
        .await
        .unwrap();
        let id_b = save_photo_message(
            &pool,
            1002,
            1,
            1002,
            Some("other".into()),
            None,
            Some("2026-08-01 09:00:00"),
        )
        .await
        .unwrap();
        let id_group = save_photo_message(
            &pool,
            -50,
            1,
            1001,
            Some("group".into()),
            None,
            Some("2026-08-01 10:00:00"),
        )
        .await
        .unwrap();

        upsert_embedding(&pool, id_a, &unit(0)).await.unwrap();
        upsert_embedding(&pool, id_b, &unit(0)).await.unwrap();
        upsert_embedding(&pool, id_group, &unit(0)).await.unwrap();

        let hits = search(
            &pool,
            a.id,
            &unit(0),
            PhotoSearchScope::Private,
            5,
            None,
            false,
        )
        .await
        .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].photo_id, id_a);
        assert_eq!(hits[0].taken_at, "2026-08-01 08:00:00");

        let group_hits = search(
            &pool,
            a.id,
            &unit(0),
            PhotoSearchScope::Chat(-50),
            5,
            None,
            false,
        )
        .await
        .unwrap();
        assert_eq!(group_hits.len(), 1);
        assert_eq!(group_hits[0].photo_id, id_group);

        let b_hits = search(
            &pool,
            b.id,
            &unit(0),
            PhotoSearchScope::Private,
            5,
            None,
            false,
        )
        .await
        .unwrap();
        assert_eq!(b_hits.len(), 1);
        assert_eq!(b_hits[0].photo_id, id_b);

        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn similar_counted_training_matches_close_vector() {
        let (pool, path) = test_db().await;
        let user = insert_user(&pool, 1101, Role::Sportive, Some("u".into()), None, None)
            .await
            .unwrap();

        let counted_id = save_photo_message(
            &pool,
            1101,
            1,
            1101,
            Some("strava".into()),
            None,
            Some("2026-08-01 08:00:00"),
        )
        .await
        .unwrap();
        upsert_embedding(&pool, counted_id, &unit(0)).await.unwrap();
        assert!(try_mark_training_counted(&pool, 1101, 1).await.unwrap());

        let new_id = save_photo_message(
            &pool,
            1101,
            2,
            1101,
            Some("strava again".into()),
            None,
            Some("2026-08-01 09:00:00"),
        )
        .await
        .unwrap();
        upsert_embedding(&pool, new_id, &unit(0)).await.unwrap();
        let photo = find_photo_message(&pool, 1101, 2).await.unwrap().unwrap();

        let hit = similar_counted_training(&pool, user.id, &photo, PhotoSearchScope::Private)
            .await
            .unwrap()
            .expect("near-identical counted photo");
        assert_eq!(hit.photo_id, counted_id);
        assert!(hit.distance <= TRAINING_DUP_MAX_DISTANCE);

        upsert_embedding(&pool, new_id, &unit(1)).await.unwrap();
        let photo = find_photo_message(&pool, 1101, 2).await.unwrap().unwrap();
        let miss = similar_counted_training(&pool, user.id, &photo, PhotoSearchScope::Private)
            .await
            .unwrap();
        assert!(miss.is_none());

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn format_pointer_context_skips_far_hits() {
        let far = PhotoHit {
            photo_id: 1,
            caption: Some("far".into()),
            taken_at: "2026-08-01 00:00:00".into(),
            distance: POINTER_MAX_DISTANCE + 0.01,
            training_counted: false,
        };
        assert!(format_pointer_context(&[far.clone()]).is_none());

        let near = PhotoHit {
            photo_id: 2,
            caption: Some("run".into()),
            taken_at: "2026-08-02 07:00:00".into(),
            distance: 0.1,
            ..far
        };
        let text = format_pointer_context(&[near]).unwrap();
        assert!(text.contains("photo_id=2"));
        assert!(text.contains("2026-08-02 07:00:00"));
        assert!(text.contains("run"));
        assert!(text.contains("Never ask for a body or physique photo"));
    }
}
