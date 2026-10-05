use anyhow::{Context, Result};
use sqlx::FromRow;

use crate::db::Db;

#[derive(Debug, FromRow)]
pub struct ToolEmbedding {
    pub embed_text: String,
    pub embedding: String,
}

/// Load a single tool embedding by name.
pub async fn find_by_name(pool: &Db, tool_name: &str) -> Result<Option<ToolEmbedding>> {
    let row = sqlx::query_as::<_, ToolEmbedding>(
        "SELECT embed_text, embedding FROM tool_embeddings WHERE tool_name = ?",
    )
    .bind(tool_name)
    .fetch_optional(pool)
    .await
    .context("failed to load tool embedding")?;

    Ok(row)
}

/// Insert or update a tool embedding.
pub async fn upsert(
    pool: &Db,
    tool_name: &str,
    embed_text: &str,
    embedding_json: &str,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO tool_embeddings (tool_name, embed_text, embedding)
        VALUES (?, ?, ?)
        ON CONFLICT(tool_name) DO UPDATE SET
            embed_text = excluded.embed_text,
            embedding = excluded.embedding,
            created_at = datetime('now')
        "#,
    )
    .bind(tool_name)
    .bind(embed_text)
    .bind(embedding_json)
    .execute(pool)
    .await
    .context("failed to upsert tool embedding")?;

    Ok(())
}
