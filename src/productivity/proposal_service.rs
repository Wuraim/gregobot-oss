use anyhow::{Result, anyhow};
use sqlx::SqlitePool;

use crate::db;
use crate::productivity::domain::{self, Proposal, ProposalWithAuthor};

pub async fn create_proposal(
    pool: &SqlitePool,
    user_id: i64,
    title: String,
    description: Option<String>,
) -> Result<Proposal> {
    if title.trim().is_empty() {
        return Err(anyhow!("Proposal title cannot be empty"));
    }

    let new = db::proposals::NewProposal {
        user_id,
        title,
        description,
    };
    let row = db::proposals::insert(pool, &new).await?;
    Ok(domain::parse_proposal(&row))
}

pub async fn list_proposals(
    pool: &SqlitePool,
    user_id: i64,
    is_admin: bool,
) -> Result<Vec<ProposalWithAuthor>> {
    if is_admin {
        let rows = db::proposals::list_all(pool, 200).await?;
        Ok(rows
            .into_iter()
            .map(|r| ProposalWithAuthor {
                id: r.id,
                user_id: r.user_id,
                title: r.title,
                description: r.description,
                created_at: domain::parse_datetime(&r.created_at).unwrap_or_default(),
                author_username: r.username,
            })
            .collect())
    } else {
        let rows = db::proposals::list_for_user(pool, user_id, 200).await?;
        Ok(rows
            .into_iter()
            .map(|r| ProposalWithAuthor {
                id: r.id,
                user_id: r.user_id,
                title: r.title,
                description: r.description,
                created_at: domain::parse_datetime(&r.created_at).unwrap_or_default(),
                author_username: None,
            })
            .collect())
    }
}

pub async fn list_my_proposals(pool: &SqlitePool, user_id: i64) -> Result<Vec<Proposal>> {
    let rows = db::proposals::list_for_user(pool, user_id, 200).await?;
    Ok(domain::parse_proposals(&rows))
}

pub async fn delete_proposal(
    pool: &SqlitePool,
    proposal_id: i64,
    user_id: i64,
    is_admin: bool,
) -> Result<bool> {
    if is_admin {
        return db::proposals::delete_any(pool, proposal_id).await;
    }

    let proposal = db::proposals::find_by_id(pool, proposal_id).await?;
    match proposal {
        Some(p) if p.user_id != user_id => Err(anyhow!("Not your proposal")),
        Some(_) => db::proposals::delete(pool, proposal_id, user_id).await,
        None => Ok(false),
    }
}
