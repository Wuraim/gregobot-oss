use anyhow::{Context, Result};
use sqlx::FromRow;

use crate::db::Db;

#[derive(Debug)]
pub struct ChunkHit {
    pub content: String,
    pub created_at: String,
}

/// Insert a chunk from a conversation message and store its embedding in the
/// vec0 virtual table. Returns the new chunk id.
pub async fn insert_conversation_chunk(
    pool: &Db,
    user_id: i64,
    chat_id: Option<i64>,
    role: &str,
    content: &str,
    source_id: i64,
    embedding: &[f32],
) -> Result<i64> {
    tracing::debug!(
        user_id,
        source_id,
        embedding_len = embedding.len(),
        "inserting conversation chunk"
    );
    let embedding_json = serialize_embedding(embedding);

    let mut tx = pool.begin().await?;

    // Use the query! macro for the regular table (validated at compile time).
    let id = sqlx::query_scalar!(
        r#"
        INSERT INTO chunks (source_type, source_id, user_id, chat_id, role, content)
        VALUES ('conversation', ?, ?, ?, ?, ?)
        RETURNING id
        "#,
        source_id,
        user_id,
        chat_id,
        role,
        content,
    )
    .fetch_one(&mut *tx)
    .await
    .context("failed to insert chunk")?
    .context("insert chunk did not return an id")?;

    // The vec0 virtual table is created at runtime, so use non-macro query.
    sqlx::query(
        r#"
        INSERT INTO chunks_vec (rowid, embedding)
        VALUES (?, ?)
        "#,
    )
    .bind(id)
    .bind(embedding_json)
    .execute(&mut *tx)
    .await
    .with_context(|| {
        format!(
            "failed to insert chunk vector (chunk_id={id}, embedding_len={})",
            embedding.len()
        )
    })?;

    tx.commit().await?;
    Ok(id)
}

/// Search the vec0 table for chunks semantically close to `query_embedding`,
/// filtered by `user_id`. Returns up to `top_k` hits.
pub async fn search(
    pool: &Db,
    user_id: i64,
    query_embedding: &[f32],
    top_k: i64,
) -> Result<Vec<ChunkHit>> {
    let query_json = serialize_embedding(query_embedding);

    // sqlite-vec requires the vector literal to appear directly in the MATCH
    // expression; binding it as a parameter does not work reliably.
    // The JSON string is generated from a slice of f32 values, so it is safe
    // to inline here (it only contains '[', ']', ',', digits, '.', '-' and 'e').
    //
    // Use a subquery to extract rowid+distance from vec0 first, then join on
    // chunks — avoids potential issues with qualified `rowid` references
    // against the virtual table inside a JOIN.
    // vec0 requires LIMIT (or `k = ?`) directly on the KNN subquery — the
    // LIMIT cannot be placed on the outer query.
    let sql = format!(
        r#"
        SELECT
            c.content AS content,
            c.created_at AS created_at
        FROM (
            SELECT rowid, distance
            FROM chunks_vec
            WHERE embedding MATCH '{query_json}'
            LIMIT ?
        ) AS v
        JOIN chunks AS c ON c.id = v.rowid
        WHERE c.user_id = ?
        ORDER BY v.distance
        "#
    );

    let rows = match sqlx::query_as::<_, ChunkHitRow>(sqlx::AssertSqlSafe(sql))
        .bind(top_k)
        .bind(user_id)
        .fetch_all(pool)
        .await
    {
        Ok(rows) => rows,
        Err(sqlx::Error::Database(db_err)) => {
            return Err(anyhow::anyhow!(
                "chunks_vec search failed: {} (dim={} rows, embedding_len={})",
                db_err.message(),
                count_rows(pool).await.unwrap_or(-1),
                query_embedding.len(),
            ));
        }
        Err(e) => return Err(e.into()),
    };

    Ok(rows
        .into_iter()
        .map(|r| ChunkHit {
            content: r.content,
            created_at: r.created_at,
        })
        .collect())
}

async fn count_rows(pool: &Db) -> Result<i64> {
    let count = sqlx::query_scalar("SELECT COUNT(*) FROM chunks_vec")
        .fetch_one(pool)
        .await
        .context("failed to count chunks_vec rows")?;
    Ok(count)
}

#[derive(FromRow)]
struct ChunkHitRow {
    content: String,
    created_at: String,
}

/// Count how many chunks a user has stored.
pub async fn count_for_user(pool: &Db, user_id: i64) -> Result<i64> {
    let count = sqlx::query_scalar!(
        r#"
        SELECT COUNT(*) FROM chunks WHERE user_id = ?
        "#,
        user_id,
    )
    .fetch_one(pool)
    .await
    .context("failed to count chunks")?;

    Ok(count)
}

/// Fetch all conversation messages that don't yet have a chunk, for backfill.
pub async fn list_unembedded_conversations(
    pool: &Db,
    user_id: i64,
    limit: i64,
) -> Result<Vec<UnembeddedConversation>> {
    let rows = sqlx::query_as!(
        UnembeddedConversation,
        r#"
        SELECT
            c.id AS id,
            c.role AS role,
            c.content AS content,
            c.chat_id AS chat_id
        FROM conversations AS c
        LEFT JOIN chunks AS ch
            ON ch.source_type = 'conversation' AND ch.source_id = c.id
        WHERE c.user_id = ? AND ch.id IS NULL
        ORDER BY c.id
        LIMIT ?
        "#,
        user_id,
        limit,
    )
    .fetch_all(pool)
    .await
    .context("failed to list unembedded conversations")?;

    Ok(rows)
}

#[derive(sqlx::FromRow)]
pub struct UnembeddedConversation {
    pub id: Option<i64>,
    pub role: String,
    pub content: String,
    pub chat_id: Option<i64>,
}

fn serialize_embedding(embedding: &[f32]) -> String {
    let mut s = String::with_capacity(2 + embedding.len() * 8);
    s.push('[');
    for (i, v) in embedding.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str(&v.to_string());
    }
    s.push(']');
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::tests::test_db;
    use crate::db::users::{Role, insert as insert_user};
    use crate::embeddings::EMBEDDING_DIMENSION;

    fn unit(i: usize) -> Vec<f32> {
        let mut v = vec![0.0f32; EMBEDDING_DIMENSION];
        v[i] = 1.0;
        v
    }

    #[tokio::test]
    async fn insert_conversation_chunk_returns_id_and_is_searchable() {
        let (pool, path) = test_db().await;
        let user = insert_user(&pool, 88001, Role::Sportive, Some("rag".into()), None, None)
            .await
            .unwrap();
        let other = insert_user(
            &pool,
            88002,
            Role::Sportive,
            Some("other".into()),
            None,
            None,
        )
        .await
        .unwrap();

        let id = insert_conversation_chunk(
            &pool,
            user.id,
            Some(-100),
            "user",
            "squat session notes",
            42,
            &unit(0),
        )
        .await
        .unwrap();
        assert!(
            id > 0,
            "RETURNING id must be a positive INTEGER PK, got {id}"
        );

        let other_id = insert_conversation_chunk(
            &pool,
            other.id,
            None,
            "user",
            "unrelated pasta recipe",
            43,
            &unit(1),
        )
        .await
        .unwrap();
        assert!(other_id > 0);
        assert_ne!(id, other_id);

        assert_eq!(count_for_user(&pool, user.id).await.unwrap(), 1);
        assert_eq!(count_for_user(&pool, other.id).await.unwrap(), 1);

        let hits = search(&pool, user.id, &unit(0), 5).await.unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].content, "squat session notes");

        let leaked = search(&pool, user.id, &unit(1), 5).await.unwrap();
        assert!(
            leaked.iter().all(|h| h.content != "unrelated pasta recipe"),
            "KNN must not return another user's chunk: {leaked:?}"
        );

        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn list_unembedded_conversations_skips_chunked_rows() {
        let (pool, path) = test_db().await;
        let user = insert_user(
            &pool,
            88003,
            Role::Sportive,
            Some("backfill".into()),
            None,
            None,
        )
        .await
        .unwrap();

        let source_id =
            crate::db::conversations::insert(&pool, user.id, "user", "need embedding", Some(-7))
                .await
                .unwrap();
        assert!(source_id > 0);

        let pending = list_unembedded_conversations(&pool, user.id, 10)
            .await
            .unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, Some(source_id));
        assert_eq!(pending[0].role, "user");
        assert_eq!(pending[0].content, "need embedding");
        assert_eq!(pending[0].chat_id, Some(-7));

        insert_conversation_chunk(
            &pool,
            user.id,
            Some(-7),
            "user",
            "need embedding",
            source_id,
            &unit(0),
        )
        .await
        .unwrap();

        let after = list_unembedded_conversations(&pool, user.id, 10)
            .await
            .unwrap();
        assert_eq!(
            after.len(),
            0,
            "chunked conversation must drop out of the backfill list"
        );

        let _ = std::fs::remove_file(&path);
    }
}
