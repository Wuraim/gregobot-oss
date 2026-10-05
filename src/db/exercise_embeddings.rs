use anyhow::{Context, Result};
use sqlx::FromRow;

use crate::db::Db;

#[derive(Debug, FromRow)]
pub struct ExerciseEmbedding {
    pub embed_text: String,
    pub embedding: String,
}

pub async fn find_by_slug(pool: &Db, slug: &str) -> Result<Option<ExerciseEmbedding>> {
    let row = sqlx::query_as::<_, ExerciseEmbedding>(
        "SELECT embed_text, embedding FROM exercise_embeddings WHERE slug = ?",
    )
    .bind(slug)
    .fetch_optional(pool)
    .await
    .context("failed to load exercise embedding")?;

    Ok(row)
}

pub async fn upsert(pool: &Db, slug: &str, embed_text: &str, embedding_json: &str) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO exercise_embeddings (slug, embed_text, embedding)
        VALUES (?, ?, ?)
        ON CONFLICT(slug) DO UPDATE SET
            embed_text = excluded.embed_text,
            embedding = excluded.embedding,
            created_at = datetime('now')
        "#,
    )
    .bind(slug)
    .bind(embed_text)
    .bind(embedding_json)
    .execute(pool)
    .await
    .context("failed to upsert exercise embedding")?;

    Ok(())
}
