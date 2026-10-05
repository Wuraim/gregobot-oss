use anyhow::{Context, Result};
use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};
use std::str::FromStr;

pub mod ai_traces;
pub mod ai_usage;
pub mod app_logs;
pub mod chat_settings;
pub mod chunks;
pub mod conversations;
pub mod daily_recap;
pub mod exercise_embeddings;
pub mod food_preferences;
pub mod foods;
pub mod goals;
pub mod group_members;
pub mod meals;
pub mod photo;
pub mod proactive;
pub mod proposals;
pub mod reminders;
pub mod sqlite_full;
pub mod strength_exercises;
pub mod tasks;
pub mod tool_embeddings;
pub mod training;
pub mod users;
pub mod weight;

pub type Db = SqlitePool;

/// Register sqlite-vec as an auto-extension so it is loaded on every SQLite
/// connection created in this process. Must be called before any connection is
/// opened (including migrations), since the `chunks_vec` virtual table relies on
/// the `vec0` module.
fn register_sqlite_extensions() {
    unsafe {
        // SAFETY: `sqlite-vec` is a trusted SQLite extension that exposes the
        // canonical entrypoint `sqlite3_vec_init`. `sqlite3_auto_extension`
        // registers it globally so every new SQLite connection auto-loads it.
        libsqlite3_sys::sqlite3_auto_extension(Some(std::mem::transmute::<
            *const (),
            unsafe extern "C" fn(
                *mut libsqlite3_sys::sqlite3,
                *mut *mut i8,
                *const libsqlite3_sys::sqlite3_api_routines,
            ) -> i32,
        >(
            sqlite_vec::sqlite3_vec_init as *const ()
        )));
    }
}

pub async fn init(database_url: &str) -> Result<Db> {
    register_sqlite_extensions();

    let opts = SqliteConnectOptions::from_str(database_url)?.create_if_missing(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(opts)
        .await
        .inspect_err(|e| tracing::error!("Failed to connect to database: {e}"))?;

    // Run pending migrations automatically on startup
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .inspect_err(|e| tracing::error!("Database migration failed: {e}"))?;

    match training::reclassify_other_activities(&pool).await {
        Ok(updated) if updated > 0 => {
            tracing::info!(
                updated,
                "Reclassified other training sessions from description"
            );
        }
        Ok(_) => {}
        Err(e) => tracing::warn!("Failed to reclassify training activities: {e}"),
    }

    // Create sqlite-vec virtual tables on startup. They cannot be created in
    // a migration because sqlx-cli cannot load the `vec0` extension.
    ensure_chunks_vec(&pool).await?;
    ensure_photo_vec(&pool).await?;

    // Prune old conversation history
    if let Err(e) = conversations::prune(&pool).await {
        tracing::warn!("Failed to prune conversation history: {e}");
    }

    match sqlite_full::prune_old_ai_usage(&pool).await {
        Ok(deleted) if deleted > 0 => {
            tracing::info!(deleted, "Pruned ai_usage rows older than 90 days");
        }
        Ok(_) => {}
        Err(e) => tracing::warn!("Failed to prune old AI usage: {e}"),
    }

    if let Err(e) = sqlite_full::checkpoint_wal(&pool).await {
        tracing::debug!("WAL checkpoint after prune failed: {e}");
    }

    Ok(pool)
}

async fn ensure_chunks_vec(pool: &Db) -> Result<()> {
    let embedding_dim = crate::embeddings::EMBEDDING_DIMENSION;

    // If the table already exists with a different dimension, recreate it.
    // This can happen when the embedding model changes.
    if let Some(declared_dim) = get_chunks_vec_dimension(pool).await?
        && declared_dim != embedding_dim
    {
        tracing::warn!(
            declared_dim,
            embedding_dim,
            "chunks_vec dimension mismatch; dropping and recreating table"
        );
        drop_chunks_vec(pool).await?;
    }

    let create_sql = format!(
        "CREATE VIRTUAL TABLE IF NOT EXISTS chunks_vec USING vec0(embedding float[{embedding_dim}])"
    );
    // Dimension is EMBEDDING_DIMENSION (compile-time usize), not user input.
    sqlx::query(sqlx::AssertSqlSafe(create_sql))
        .execute(pool)
        .await
        .context("failed to create chunks_vec virtual table")?;
    Ok(())
}

async fn ensure_photo_vec(pool: &Db) -> Result<()> {
    let embedding_dim = crate::embeddings::EMBEDDING_DIMENSION;

    if let Some(declared_dim) = get_vec_dimension(pool, "photo_vec").await?
        && declared_dim != embedding_dim
    {
        tracing::warn!(
            declared_dim,
            embedding_dim,
            "photo_vec dimension mismatch; dropping and recreating table"
        );
        sqlx::query("DROP TABLE IF EXISTS photo_vec")
            .execute(pool)
            .await
            .context("failed to drop photo_vec")?;
        sqlx::query("UPDATE photo_messages SET embedding = NULL")
            .execute(pool)
            .await
            .context("failed to clear photo embeddings")?;
    }

    let create_sql = format!(
        "CREATE VIRTUAL TABLE IF NOT EXISTS photo_vec USING vec0(embedding float[{embedding_dim}])"
    );
    // Dimension is EMBEDDING_DIMENSION (compile-time usize), not user input.
    sqlx::query(sqlx::AssertSqlSafe(create_sql))
        .execute(pool)
        .await
        .context("failed to create photo_vec virtual table")?;
    Ok(())
}

/// Read the declared embedding dimension from an existing vec0 table schema.
async fn get_chunks_vec_dimension(pool: &Db) -> Result<Option<usize>> {
    get_vec_dimension(pool, "chunks_vec").await
}

async fn get_vec_dimension(pool: &Db, table: &str) -> Result<Option<usize>> {
    let sql: Option<String> =
        sqlx::query_scalar("SELECT sql FROM sqlite_master WHERE name = ? AND type = 'table'")
            .bind(table)
            .fetch_optional(pool)
            .await
            .with_context(|| format!("failed to read {table} schema"))?;

    let Some(sql) = sql else {
        return Ok(None);
    };
    parse_vec_dimension(&sql)
}

fn parse_vec_dimension(sql: &str) -> Result<Option<usize>> {
    let Some(start) = sql.find("float[") else {
        return Ok(None);
    };
    let rest = &sql[start + "float[".len()..];
    let Some(end) = rest.find(']') else {
        return Ok(None);
    };
    rest[..end]
        .trim()
        .parse::<usize>()
        .map(Some)
        .context("failed to parse chunks_vec dimension")
}

async fn drop_chunks_vec(pool: &Db) -> Result<()> {
    sqlx::query("DROP TABLE IF EXISTS chunks_vec")
        .execute(pool)
        .await
        .context("failed to drop chunks_vec")?;
    // Vectors are gone, so the associated chunks metadata is useless too.
    sqlx::query("DELETE FROM chunks")
        .execute(pool)
        .await
        .context("failed to clear chunks table")?;
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// Create a unique real SQLite file and run migrations.
    ///
    /// The caller is responsible for deleting the file when the test ends.
    /// Using `std::fs::remove_file` is safe even if the pool is still open on
    /// Linux/WSL because SQLite keeps the file descriptor alive after unlink.
    pub async fn test_db() -> (Db, PathBuf) {
        let tmp = std::env::temp_dir();
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let pid = std::process::id();
        let path = tmp.join(format!("gregobot_test_{pid}_{ts}.db"));
        let url = format!("sqlite:{}", path.display());

        let pool = init(&url)
            .await
            .expect("failed to initialize test database");
        (pool, path)
    }

    #[test]
    fn parse_vec_dimension_standard() {
        let sql = "CREATE VIRTUAL TABLE chunks_vec USING vec0(embedding float[768])";
        assert_eq!(parse_vec_dimension(sql).unwrap(), Some(768));
    }

    #[test]
    fn parse_vec_dimension_with_whitespace() {
        let sql = "CREATE VIRTUAL TABLE chunks_vec USING vec0(embedding float[ 1536 ])";
        assert_eq!(parse_vec_dimension(sql).unwrap(), Some(1536));
    }

    #[test]
    fn parse_vec_dimension_missing() {
        let sql = "CREATE TABLE chunks_vec (id INTEGER PRIMARY KEY)";
        assert_eq!(parse_vec_dimension(sql).unwrap(), None);
    }

    #[tokio::test]
    async fn init_creates_vec0_virtual_tables() {
        let (pool, path) = test_db().await;
        let chunks: String = sqlx::query_scalar(
            "SELECT sql FROM sqlite_master WHERE name = 'chunks_vec' AND type = 'table'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(
            chunks.contains("vec0"),
            "chunks_vec must be a sqlite-vec virtual table: {chunks}"
        );
        let photos: String = sqlx::query_scalar(
            "SELECT sql FROM sqlite_master WHERE name = 'photo_vec' AND type = 'table'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(
            photos.contains("vec0"),
            "photo_vec must be a sqlite-vec virtual table: {photos}"
        );
        let _ = std::fs::remove_file(&path);
    }
}
