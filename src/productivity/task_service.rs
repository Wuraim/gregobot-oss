use anyhow::{Result, anyhow};
use chrono::{DateTime, Utc};
use sqlx::SqlitePool;

use crate::db;
use crate::productivity::domain::{self, Task, TaskPriority, TaskSource, TaskStatus};

pub struct CreateTaskInput {
    pub user_id: i64,
    pub title: String,
    pub description: Option<String>,
    pub priority: TaskPriority,
    pub due_at: Option<DateTime<Utc>>,
    pub tags: Vec<String>,
    pub source: TaskSource,
}

pub async fn create_task(pool: &SqlitePool, input: CreateTaskInput) -> Result<Task> {
    if input.title.trim().is_empty() {
        return Err(anyhow!("Task title cannot be empty"));
    }

    let new = db::tasks::NewTask {
        user_id: input.user_id,
        title: input.title,
        description: input.description,
        priority: input.priority,
        due_at: input.due_at,
        tags: input
            .tags
            .into_iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
        source: input.source,
    };

    let row = db::tasks::insert(pool, &new).await?;
    Ok(domain::parse_task(&row))
}

pub struct UpdateTask {
    pub title: Option<String>,
    pub description: Option<String>,
    pub priority: Option<TaskPriority>,
    pub due_at: Option<Option<DateTime<Utc>>>,
    pub tags: Option<Vec<String>>,
}

pub async fn update_task(
    pool: &SqlitePool,
    task_id: i64,
    user_id: i64,
    updates: UpdateTask,
) -> Result<Task> {
    let existing = db::tasks::find_by_id(pool, task_id)
        .await?
        .ok_or_else(|| anyhow!("Task not found"))?;
    if existing.user_id != user_id {
        return Err(anyhow!("Not your task"));
    }

    let updated = db::tasks::Task {
        id: existing.id,
        user_id: existing.user_id,
        title: updates.title.unwrap_or(existing.title),
        description: updates.description.or(existing.description),
        status: existing.status,
        priority: updates
            .priority
            .map(|p| p.as_str().to_string())
            .unwrap_or(existing.priority),
        due_at: match updates.due_at {
            Some(Some(dt)) => Some(dt.to_rfc3339()),
            Some(None) => None,
            None => existing.due_at,
        },
        tags: updates.tags.map(|t| t.join(",")).unwrap_or(existing.tags),
        source: existing.source,
        created_at: existing.created_at,
        completed_at: existing.completed_at,
    };

    db::tasks::update(pool, &updated).await?;

    let row = db::tasks::find_by_id(pool, task_id)
        .await?
        .ok_or_else(|| anyhow!("Task disappeared after update"))?;
    Ok(domain::parse_task(&row))
}

pub async fn update_status(
    pool: &SqlitePool,
    task_id: i64,
    user_id: i64,
    new_status: TaskStatus,
) -> Result<Task> {
    let task = db::tasks::find_by_id(pool, task_id)
        .await?
        .ok_or_else(|| anyhow!("Task not found"))?;
    if task.user_id != user_id {
        return Err(anyhow!("Not your task"));
    }

    db::tasks::update_status(pool, task_id, user_id, new_status).await?;

    // If task is completed/cancelled, cancel any pending reminders targeting it
    if matches!(new_status, TaskStatus::Done | TaskStatus::Cancelled) {
        let _ = db::reminders::cancel_for_target(pool, "task", task_id).await;
    }

    let updated = db::tasks::find_by_id(pool, task_id).await?.unwrap();
    Ok(domain::parse_task(&updated))
}

pub async fn list_tasks(
    pool: &SqlitePool,
    user_id: i64,
    status_filter: Option<TaskStatus>,
) -> Result<Vec<Task>> {
    let rows = db::tasks::list_for_user(pool, user_id, status_filter, 100).await?;
    Ok(domain::parse_tasks(&rows))
}

pub async fn delete_task(pool: &SqlitePool, task_id: i64, user_id: i64) -> Result<bool> {
    let task = match db::tasks::find_by_id(pool, task_id).await? {
        Some(t) => t,
        None => return Ok(false),
    };
    if task.user_id != user_id {
        return Err(anyhow!("Not your task"));
    }

    let _ = db::reminders::cancel_for_target(pool, "task", task_id).await;

    db::tasks::delete(pool, task_id, user_id).await
}
