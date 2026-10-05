use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    Todo,
    Doing,
    Done,
    Cancelled,
}

impl TaskStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Todo => "todo",
            Self::Doing => "doing",
            Self::Done => "done",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "todo" => Some(Self::Todo),
            "doing" => Some(Self::Doing),
            "done" => Some(Self::Done),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskPriority {
    Low,
    Medium,
    High,
}

impl TaskPriority {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "low" => Some(Self::Low),
            "medium" => Some(Self::Medium),
            "high" => Some(Self::High),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskSource {
    Manual,
    AiPlanner,
}

impl TaskSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::AiPlanner => "ai_planner",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "manual" => Some(Self::Manual),
            "ai_planner" => Some(Self::AiPlanner),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: i64,
    pub user_id: i64,
    pub title: String,
    pub description: Option<String>,
    pub status: String,
    pub priority: String,
    pub due_at: Option<String>,
    pub tags: String,
    pub source: String,
    pub created_at: String,
    pub completed_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct NewTask {
    pub user_id: i64,
    pub title: String,
    pub description: Option<String>,
    pub priority: TaskPriority,
    pub due_at: Option<DateTime<Utc>>,
    pub tags: Vec<String>,
    pub source: TaskSource,
}

pub async fn insert(pool: &SqlitePool, new: &NewTask) -> Result<Task> {
    let due_at_str = new.due_at.map(|d| d.to_rfc3339());
    let tags_str = new.tags.join(",");
    let source_str = new.source.as_str();
    let priority_str = new.priority.as_str();

    let id = sqlx::query!(
        r#"
        INSERT INTO tasks (user_id, title, description, status, priority, due_at, tags, source, completed_at)
        VALUES (?, ?, ?, 'todo', ?, ?, ?, ?, NULL)
        "#,
        new.user_id,
        new.title,
        new.description,
        priority_str,
        due_at_str,
        tags_str,
        source_str,
    )
    .execute(pool)
    .await?
    .last_insert_rowid();

    let row = sqlx::query!(
        r#"
        SELECT id, user_id, title, description, status, priority, due_at, tags, source, created_at, completed_at
        FROM tasks WHERE id = ?
        "#,
        id,
    )
    .fetch_one(pool)
    .await?;

    Ok(Task {
        id: row.id,
        user_id: row.user_id,
        title: row.title,
        description: row.description,
        status: row.status,
        priority: row.priority,
        due_at: row.due_at,
        tags: row.tags.unwrap_or_default(),
        source: row.source,
        created_at: row.created_at,
        completed_at: row.completed_at,
    })
}

pub async fn find_by_id(pool: &SqlitePool, id: i64) -> Result<Option<Task>> {
    let row = sqlx::query!(
        r#"
        SELECT id, user_id, title, description, status, priority, due_at, tags, source, created_at, completed_at
        FROM tasks WHERE id = ?
        "#,
        id,
    )
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| Task {
        id: r.id,
        user_id: r.user_id,
        title: r.title,
        description: r.description,
        status: r.status,
        priority: r.priority,
        due_at: r.due_at,
        tags: r.tags.unwrap_or_default(),
        source: r.source,
        created_at: r.created_at,
        completed_at: r.completed_at,
    }))
}

pub async fn list_for_user(
    pool: &SqlitePool,
    user_id: i64,
    status: Option<TaskStatus>,
    limit: i64,
) -> Result<Vec<Task>> {
    if let Some(status) = status {
        let status_str = status.as_str();
        let rows = sqlx::query!(
            r#"
            SELECT id, user_id, title, description, status, priority, due_at, tags, source, created_at, completed_at
            FROM tasks
            WHERE user_id = ? AND status = ?
            ORDER BY created_at DESC
            LIMIT ?
            "#,
            user_id,
            status_str,
            limit,
        )
        .fetch_all(pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| Task {
                id: r.id.expect("id"),
                user_id: r.user_id,
                title: r.title,
                description: r.description,
                status: r.status,
                priority: r.priority,
                due_at: r.due_at,
                tags: r.tags.unwrap_or_default(),
                source: r.source,
                created_at: r.created_at,
                completed_at: r.completed_at,
            })
            .collect())
    } else {
        let rows = sqlx::query!(
            r#"
            SELECT id, user_id, title, description, status, priority, due_at, tags, source, created_at, completed_at
            FROM tasks
            WHERE user_id = ?
            ORDER BY created_at DESC
            LIMIT ?
            "#,
            user_id,
            limit,
        )
        .fetch_all(pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| Task {
                id: r.id.expect("id"),
                user_id: r.user_id,
                title: r.title,
                description: r.description,
                status: r.status,
                priority: r.priority,
                due_at: r.due_at,
                tags: r.tags.unwrap_or_default(),
                source: r.source,
                created_at: r.created_at,
                completed_at: r.completed_at,
            })
            .collect())
    }
}

pub async fn update_status(
    pool: &SqlitePool,
    id: i64,
    user_id: i64,
    status: TaskStatus,
) -> Result<bool> {
    let status_str = status.as_str();

    let result = sqlx::query!(
        r#"
        UPDATE tasks
        SET status = ?,
            completed_at = CASE WHEN ? = 'done' THEN datetime('now') ELSE NULL END
        WHERE id = ? AND user_id = ?
        "#,
        status_str,
        status_str,
        id,
        user_id,
    )
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

pub async fn update(pool: &SqlitePool, task: &Task) -> Result<bool> {
    let result = sqlx::query!(
        r#"
        UPDATE tasks
        SET title = ?, description = ?, priority = ?, due_at = ?, tags = ?
        WHERE id = ? AND user_id = ?
        "#,
        task.title,
        task.description,
        task.priority,
        task.due_at,
        task.tags,
        task.id,
        task.user_id,
    )
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

pub async fn delete(pool: &SqlitePool, id: i64, user_id: i64) -> Result<bool> {
    let result = sqlx::query!(
        "DELETE FROM tasks WHERE id = ? AND user_id = ?",
        id,
        user_id,
    )
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_status_round_trip() {
        for variant in [
            TaskStatus::Todo,
            TaskStatus::Doing,
            TaskStatus::Done,
            TaskStatus::Cancelled,
        ] {
            assert_eq!(TaskStatus::from_str(variant.as_str()), Some(variant));
        }
        assert_eq!(TaskStatus::from_str("unknown"), None);
    }

    #[test]
    fn task_priority_round_trip() {
        for variant in [TaskPriority::Low, TaskPriority::Medium, TaskPriority::High] {
            assert_eq!(TaskPriority::from_str(variant.as_str()), Some(variant));
        }
    }

    #[test]
    fn task_source_round_trip() {
        for variant in [TaskSource::Manual, TaskSource::AiPlanner] {
            assert_eq!(TaskSource::from_str(variant.as_str()), Some(variant));
        }
    }

    #[tokio::test]
    async fn update_status_and_delete_task() {
        use crate::db::tests::test_db;
        use crate::db::users::{Role, insert as insert_user};
        use crate::productivity::task_service;

        let (pool, path) = test_db().await;
        let user = insert_user(
            &pool,
            99001,
            Role::Sportive,
            Some("tester".into()),
            None,
            None,
        )
        .await
        .unwrap();
        let task = task_service::create_task(
            &pool,
            task_service::CreateTaskInput {
                user_id: user.id,
                title: "Workout".into(),
                description: Some("legs day".into()),
                priority: TaskPriority::High,
                due_at: None,
                tags: vec!["gym".into()],
                source: TaskSource::Manual,
            },
        )
        .await
        .unwrap();
        assert_eq!(task.title, "Workout");
        assert_eq!(task.priority, TaskPriority::High);

        assert!(
            update_status(&pool, task.id, user.id, TaskStatus::Done)
                .await
                .unwrap()
        );

        let list = list_for_user(&pool, user.id, None, 10).await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].status, TaskStatus::Done.as_str());

        assert!(delete(&pool, task.id, user.id).await.unwrap());
        let list = list_for_user(&pool, user.id, None, 10).await.unwrap();
        assert!(list.is_empty());
        let _ = std::fs::remove_file(&path);
    }
}
