use anyhow::Result;
use sqlx::SqlitePool;

#[derive(Debug, Clone)]
pub struct Proposal {
    pub id: i64,
    pub user_id: i64,
    pub title: String,
    pub description: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

pub struct NewProposal {
    pub user_id: i64,
    pub title: String,
    pub description: Option<String>,
}

pub async fn insert(pool: &SqlitePool, new: &NewProposal) -> Result<Proposal> {
    let id = sqlx::query!(
        "INSERT INTO feature_proposals (user_id, title, description) VALUES (?, ?, ?)",
        new.user_id,
        new.title,
        new.description,
    )
    .execute(pool)
    .await?
    .last_insert_rowid();

    Ok(find_by_id(pool, id).await?.expect("just inserted"))
}

pub async fn find_by_id(pool: &SqlitePool, id: i64) -> Result<Option<Proposal>> {
    let row = sqlx::query!(
        "SELECT id, user_id, title, description, created_at, updated_at FROM feature_proposals WHERE id = ?",
        id,
    )
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| Proposal {
        id: r.id,
        user_id: r.user_id,
        title: r.title,
        description: r.description,
        created_at: r.created_at,
        updated_at: r.updated_at,
    }))
}

pub async fn list_for_user(pool: &SqlitePool, user_id: i64, limit: i64) -> Result<Vec<Proposal>> {
    let rows = sqlx::query!(
        "SELECT id, user_id, title, description, created_at, updated_at FROM feature_proposals WHERE user_id = ? ORDER BY created_at DESC LIMIT ?",
        user_id,
        limit,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| Proposal {
            id: r.id.expect("id"),
            user_id: r.user_id,
            title: r.title,
            description: r.description,
            created_at: r.created_at,
            updated_at: r.updated_at,
        })
        .collect())
}

pub struct ProposalWithUsername {
    pub id: i64,
    pub user_id: i64,
    pub title: String,
    pub description: Option<String>,
    pub created_at: String,
    pub username: Option<String>,
}

pub async fn list_all(pool: &SqlitePool, limit: i64) -> Result<Vec<ProposalWithUsername>> {
    let rows = sqlx::query!(
        r#"SELECT fp.id, fp.user_id, fp.title, fp.description, fp.created_at, u.username
           FROM feature_proposals fp
           JOIN users u ON u.id = fp.user_id
           ORDER BY fp.created_at DESC LIMIT ?"#,
        limit,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| ProposalWithUsername {
            id: r.id,
            user_id: r.user_id,
            title: r.title,
            description: r.description,
            created_at: r.created_at,
            username: r.username,
        })
        .collect())
}

pub async fn delete(pool: &SqlitePool, id: i64, user_id: i64) -> Result<bool> {
    let result = sqlx::query!(
        "DELETE FROM feature_proposals WHERE id = ? AND user_id = ?",
        id,
        user_id,
    )
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

pub async fn delete_any(pool: &SqlitePool, id: i64) -> Result<bool> {
    let result = sqlx::query!("DELETE FROM feature_proposals WHERE id = ?", id)
        .execute(pool)
        .await?;

    Ok(result.rows_affected() > 0)
}
