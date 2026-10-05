use axum::{
    Json, Router,
    extract::{Path, Query, Request, State},
    http::{HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::{delete, get, patch, post},
};
use serde::{Deserialize, Serialize};

use crate::ai::log::AiTraceFilter;
use crate::db::{self, users::Role};
use crate::logs::{LogFilter, SharedLogBuffer};

#[derive(Clone)]
pub struct AppState {
    pub pool: crate::db::Db,
    pub logs: SharedLogBuffer,
    /// REST admin API key checked via `X-API-Key` (from `ADMIN_API_KEY` or `BOT_TOKEN`).
    pub admin_api_key: String,
    /// Gemini API token for catalog embeddings after REST food create/update.
    pub gemini_api_token: Option<String>,
    pub gemini_embedding_model: String,
}

pub fn router(state: AppState) -> Router {
    let api = Router::new()
        // Weight routes
        .route("/api/weight/{telegram_user_id}", get(list_weight))
        .route("/api/weight/{telegram_user_id}/latest", get(latest_weight))
        .route("/api/weight", post(create_weight))
        // Training routes
        .route("/api/training/{telegram_user_id}", get(list_training))
        .route("/api/training", post(create_training))
        // User management routes
        .route("/api/users", get(list_users))
        .route("/api/users", post(create_user))
        .route("/api/users/{telegram_user_id}", delete(delete_user))
        // Logs routes
        .route("/api/logs", get(get_logs))
        // AI trace routes
        .route("/api/ai/logs", get(get_ai_logs))
        .route("/api/ai/logs/{id}", get(get_ai_log))
        .route("/api/ai/pricing", get(get_ai_pricing))
        // Task routes
        .route("/api/tasks/{telegram_user_id}", get(list_tasks))
        .route("/api/tasks", post(create_task))
        .route(
            "/api/tasks/{telegram_user_id}/{task_id}/done",
            post(mark_task_done),
        )
        .route(
            "/api/tasks/{telegram_user_id}/{task_id}/cancel",
            post(cancel_task),
        )
        .route(
            "/api/tasks/{telegram_user_id}/{task_id}",
            delete(delete_task),
        )
        // Reminder routes
        .route("/api/reminders/{telegram_user_id}", get(list_reminders))
        .route("/api/reminders", post(create_reminder_api))
        .route(
            "/api/reminders/{telegram_user_id}/{reminder_id}",
            delete(cancel_reminder),
        )
        // Proposal routes
        .route("/api/proposals/{telegram_user_id}", get(list_proposals_api))
        .route("/api/proposals", post(create_proposal_api))
        .route(
            "/api/proposals/{telegram_user_id}/{proposal_id}",
            delete(delete_proposal_api),
        )
        // Nutrition
        .route("/api/foods/{telegram_user_id}", get(list_foods))
        .route("/api/foods", post(create_food))
        .route(
            "/api/foods/{telegram_user_id}/{food_id}",
            patch(update_food).delete(delete_food),
        )
        .route("/api/meals/{telegram_user_id}", get(list_meals))
        .route("/api/meals", post(create_meal))
        .route(
            "/api/meals/{telegram_user_id}/{meal_id}",
            delete(delete_meal),
        )
        .route("/api/nutrition/{telegram_user_id}", get(get_day_nutrition))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            require_api_key,
        ));

    Router::new()
        .route("/health", get(health))
        .route("/logs", get(log_viewer))
        .route("/ai/logs", get(ai_log_viewer))
        .merge(api)
        .with_state(state)
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Resolve a telegram_user_id path param to an internal `users.id`.
async fn resolve_user_id(pool: &crate::db::Db, telegram_user_id: i64) -> Result<i64, Response> {
    match db::users::find_by_telegram_id(pool, telegram_user_id).await {
        Ok(Some(u)) => Ok(u.id),
        Ok(None) => Err((StatusCode::NOT_FOUND, "User not found").into_response()),
        Err(err) => {
            tracing::error!("resolve_user_id error: {err}");
            Err((StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response())
        }
    }
}

// ── Weight handlers ───────────────────────────────────────────────────────────

async fn list_weight(State(state): State<AppState>, Path(telegram_user_id): Path<i64>) -> Response {
    let user_id = match resolve_user_id(&state.pool, telegram_user_id).await {
        Ok(id) => id,
        Err(resp) => return resp,
    };

    match db::weight::list_for_user(&state.pool, user_id).await {
        Ok(entries) => Json(entries).into_response(),
        Err(err) => {
            tracing::error!("list_weight error: {err}");
            (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response()
        }
    }
}

async fn latest_weight(
    State(state): State<AppState>,
    Path(telegram_user_id): Path<i64>,
) -> Response {
    let user_id = match resolve_user_id(&state.pool, telegram_user_id).await {
        Ok(id) => id,
        Err(resp) => return resp,
    };

    match db::weight::latest_for_user(&state.pool, user_id).await {
        Ok(Some(entry)) => Json(entry).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "No weight entries found").into_response(),
        Err(err) => {
            tracing::error!("latest_weight error: {err}");
            (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response()
        }
    }
}

#[derive(Deserialize)]
struct CreateWeight {
    telegram_user_id: i64,
    weight_kg: f64,
}

async fn create_weight(State(state): State<AppState>, Json(body): Json<CreateWeight>) -> Response {
    let user_id = match resolve_user_id(&state.pool, body.telegram_user_id).await {
        Ok(id) => id,
        Err(resp) => return resp,
    };

    match db::weight::insert(
        &state.pool,
        db::weight::NewWeightEntry {
            user_id,
            weight_kg: body.weight_kg,
            recorded_at: None,
        },
    )
    .await
    {
        Ok(e) => (StatusCode::CREATED, Json(e)).into_response(),
        Err(err) => {
            tracing::error!("create_weight error: {err}");
            (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response()
        }
    }
}

// ── Training handlers ─────────────────────────────────────────────────────────

async fn list_training(
    State(state): State<AppState>,
    Path(telegram_user_id): Path<i64>,
) -> Response {
    let user_id = match resolve_user_id(&state.pool, telegram_user_id).await {
        Ok(id) => id,
        Err(resp) => return resp,
    };

    match db::training::list_for_user(&state.pool, user_id).await {
        Ok(entries) => Json(entries).into_response(),
        Err(err) => {
            tracing::error!("list_training error: {err}");
            (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response()
        }
    }
}

#[derive(Deserialize)]
struct CreateTraining {
    telegram_user_id: i64,
    description: String,
}

async fn create_training(
    State(state): State<AppState>,
    Json(body): Json<CreateTraining>,
) -> Response {
    let user_id = match resolve_user_id(&state.pool, body.telegram_user_id).await {
        Ok(id) => id,
        Err(resp) => return resp,
    };

    match db::training::try_insert(
        &state.pool,
        db::training::NewTrainingEntry::simple(user_id, body.description, None),
    )
    .await
    {
        Ok(db::training::TrainingInsert::Created(e)) => {
            (StatusCode::CREATED, Json(e)).into_response()
        }
        Ok(db::training::TrainingInsert::Duplicate(e)) => (StatusCode::OK, Json(e)).into_response(),
        Err(err) => {
            tracing::error!("create_training error: {err}");
            (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response()
        }
    }
}

// ── User management handlers ──────────────────────────────────────────────────

async fn list_users(State(state): State<AppState>) -> Response {
    match db::users::list(&state.pool).await {
        Ok(users) => Json(users).into_response(),
        Err(err) => {
            tracing::error!("list_users error: {err}");
            (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response()
        }
    }
}

#[derive(Deserialize)]
struct CreateUser {
    telegram_user_id: i64,
    role: Role,
    username: Option<String>,
    language_code: Option<String>,
}

async fn create_user(State(state): State<AppState>, Json(body): Json<CreateUser>) -> Response {
    match db::users::insert(
        &state.pool,
        body.telegram_user_id,
        body.role,
        body.username,
        body.language_code,
        None,
    )
    .await
    {
        Ok(user) => (StatusCode::CREATED, Json(user)).into_response(),
        Err(err) => {
            tracing::error!("create_user error: {err}");
            // Likely a UNIQUE constraint violation
            (StatusCode::CONFLICT, "User already exists or invalid role").into_response()
        }
    }
}

async fn delete_user(State(state): State<AppState>, Path(telegram_user_id): Path<i64>) -> Response {
    match db::users::delete(&state.pool, telegram_user_id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (StatusCode::NOT_FOUND, "User not found").into_response(),
        Err(err) => {
            tracing::error!("delete_user error: {err}");
            (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response()
        }
    }
}

// ── Logs ─────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct LogParams {
    lines: Option<usize>,
    /// Comma-separated tracing levels, e.g. `ERROR,WARN`.
    level: Option<String>,
    target: Option<String>,
    q: Option<String>,
    /// `json` (default) or `text`.
    format: Option<String>,
}

async fn get_logs(State(state): State<AppState>, Query(params): Query<LogParams>) -> Response {
    let lines = params.lines.unwrap_or(100).min(state.logs.max_entries());
    let filter = LogFilter::from_query(
        params.level.as_deref(),
        params.target.as_deref(),
        params.q.as_deref(),
    );
    let entries = state.logs.snapshot_filtered(lines, &filter);
    if params
        .format
        .as_deref()
        .is_some_and(|f| f.eq_ignore_ascii_case("text"))
    {
        let body = entries
            .iter()
            .map(|e| format!("{} {} {} {}", e.timestamp, e.level, e.target, e.message))
            .collect::<Vec<_>>()
            .join("\n");
        return (
            [(
                axum::http::header::CONTENT_TYPE,
                "text/plain; charset=utf-8",
            )],
            body,
        )
            .into_response();
    }
    Json(entries).into_response()
}

async fn log_viewer() -> Html<&'static str> {
    Html(include_str!("logs_viewer.html"))
}

// ── AI trace handlers ─────────────────────────────────────────────────────────

fn check_api_key(state: &AppState, headers: &HeaderMap) -> bool {
    let provided_key = headers.get("X-API-Key").and_then(|v| v.to_str().ok());
    provided_key == Some(&state.admin_api_key)
}

async fn require_api_key(State(state): State<AppState>, req: Request, next: Next) -> Response {
    if !check_api_key(&state, req.headers()) {
        return (StatusCode::UNAUTHORIZED, "Invalid API key").into_response();
    }
    next.run(req).await
}

#[derive(Deserialize)]
struct AiLogParams {
    limit: Option<usize>,
    error: Option<bool>,
    telegram_user_id: Option<i64>,
    kind: Option<String>,
}

async fn get_ai_logs(Query(params): Query<AiLogParams>) -> Response {
    let limit = params
        .limit
        .unwrap_or(50)
        .min(crate::ai::log::global().max_entries());
    let filter = AiTraceFilter {
        error: params.error,
        telegram_user_id: params.telegram_user_id,
        kind: params.kind.filter(|s| !s.is_empty()),
    };
    Json(crate::ai::log::global().summaries_filtered(limit, &filter)).into_response()
}

async fn get_ai_log(Path(id): Path<u64>) -> Response {
    match crate::ai::log::global().get(id) {
        Some(trace) => Json(trace).into_response(),
        None => (StatusCode::NOT_FOUND, "Trace not found").into_response(),
    }
}

async fn get_ai_pricing() -> Response {
    Json(crate::ai::pricing::pricing_catalog()).into_response()
}

async fn ai_log_viewer() -> Html<&'static str> {
    Html(include_str!("ai_logs_viewer.html"))
}

// ── Task handlers ──────────────────────────────────────────────────────────────

async fn list_tasks(State(state): State<AppState>, Path(telegram_user_id): Path<i64>) -> Response {
    let user_id = match resolve_user_id(&state.pool, telegram_user_id).await {
        Ok(id) => id,
        Err(resp) => return resp,
    };

    let tasks =
        match crate::productivity::task_service::list_tasks(&state.pool, user_id, None).await {
            Ok(t) => t,
            Err(err) => {
                tracing::error!("list_tasks error: {err}");
                return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
            }
        };
    Json(tasks).into_response()
}

#[derive(Deserialize)]
struct CreateTaskBody {
    telegram_user_id: i64,
    title: String,
    priority: Option<String>,
    due_at: Option<String>,
    tags: Option<Vec<String>>,
}

async fn create_task(State(state): State<AppState>, Json(body): Json<CreateTaskBody>) -> Response {
    let user_id = match resolve_user_id(&state.pool, body.telegram_user_id).await {
        Ok(id) => id,
        Err(resp) => return resp,
    };

    let priority = match body.priority.as_deref() {
        Some("high") => crate::productivity::domain::TaskPriority::High,
        Some("medium") | None => crate::productivity::domain::TaskPriority::Medium,
        Some("low") => crate::productivity::domain::TaskPriority::Low,
        _ => crate::productivity::domain::TaskPriority::Medium,
    };

    let due_at = body.due_at.as_deref().and_then(|s| {
        chrono::DateTime::parse_from_rfc3339(s)
            .ok()
            .map(|d| d.with_timezone(&chrono::Utc))
    });

    match crate::productivity::task_service::create_task(
        &state.pool,
        crate::productivity::task_service::CreateTaskInput {
            user_id,
            title: body.title,
            description: None,
            priority,
            due_at,
            tags: body.tags.unwrap_or_default(),
            source: crate::productivity::domain::TaskSource::Manual,
        },
    )
    .await
    {
        Ok(task) => (StatusCode::CREATED, Json(task)).into_response(),
        Err(err) => {
            tracing::error!("create_task error: {err}");
            (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response()
        }
    }
}

async fn mark_task_done(
    State(state): State<AppState>,
    Path((telegram_user_id, task_id)): Path<(i64, i64)>,
) -> Response {
    let user_id = match resolve_user_id(&state.pool, telegram_user_id).await {
        Ok(id) => id,
        Err(resp) => return resp,
    };

    match crate::productivity::task_service::update_status(
        &state.pool,
        task_id,
        user_id,
        crate::productivity::domain::TaskStatus::Done,
    )
    .await
    {
        Ok(task) => Json(task).into_response(),
        Err(err) => {
            tracing::error!("mark_task_done error: {err}");
            (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response()
        }
    }
}

async fn cancel_task(
    State(state): State<AppState>,
    Path((telegram_user_id, task_id)): Path<(i64, i64)>,
) -> Response {
    let user_id = match resolve_user_id(&state.pool, telegram_user_id).await {
        Ok(id) => id,
        Err(resp) => return resp,
    };

    match crate::productivity::task_service::update_status(
        &state.pool,
        task_id,
        user_id,
        crate::productivity::domain::TaskStatus::Cancelled,
    )
    .await
    {
        Ok(task) => Json(task).into_response(),
        Err(err) => {
            tracing::error!("cancel_task error: {err}");
            (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response()
        }
    }
}

async fn delete_task(
    State(state): State<AppState>,
    Path((telegram_user_id, task_id)): Path<(i64, i64)>,
) -> Response {
    let user_id = match resolve_user_id(&state.pool, telegram_user_id).await {
        Ok(id) => id,
        Err(resp) => return resp,
    };

    match crate::productivity::task_service::delete_task(&state.pool, task_id, user_id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (StatusCode::NOT_FOUND, "Task not found").into_response(),
        Err(err) => {
            tracing::error!("delete_task error: {err}");
            (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response()
        }
    }
}

// ── Reminder handlers ──────────────────────────────────────────────────────────

async fn list_reminders(
    State(state): State<AppState>,
    Path(telegram_user_id): Path<i64>,
) -> Response {
    let user_id = match resolve_user_id(&state.pool, telegram_user_id).await {
        Ok(id) => id,
        Err(resp) => return resp,
    };

    let reminders =
        match crate::productivity::reminder_service::list_reminders(&state.pool, user_id, None)
            .await
        {
            Ok(r) => r,
            Err(err) => {
                tracing::error!("list_reminders error: {err}");
                return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
            }
        };
    Json(reminders).into_response()
}

#[derive(Deserialize)]
struct CreateReminderBody {
    telegram_user_id: i64,
    scheduled_at: String,
    message: Option<String>,
    target_type: Option<String>,
    target_id: Option<i64>,
}

async fn create_reminder_api(
    State(state): State<AppState>,
    Json(body): Json<CreateReminderBody>,
) -> Response {
    let user_id = match resolve_user_id(&state.pool, body.telegram_user_id).await {
        Ok(id) => id,
        Err(resp) => return resp,
    };

    let scheduled_at = match chrono::DateTime::parse_from_rfc3339(&body.scheduled_at)
        .map(|d| d.with_timezone(&chrono::Utc))
    {
        Ok(d) => d,
        Err(_) => return (StatusCode::BAD_REQUEST, "Invalid scheduled_at format").into_response(),
    };

    let target = match body.target_type.as_deref() {
        Some("task") => crate::productivity::domain::ReminderTarget::Task {
            task_id: body.target_id.unwrap_or(0),
        },
        _ => crate::productivity::domain::ReminderTarget::Custom {
            message: body.message.unwrap_or_default(),
        },
    };

    match crate::productivity::reminder_service::create_reminder(
        &state.pool,
        user_id,
        target,
        scheduled_at,
        crate::productivity::domain::ReminderRecurrence::None,
    )
    .await
    {
        Ok(reminder) => (StatusCode::CREATED, Json(reminder)).into_response(),
        Err(err) => {
            tracing::error!("create_reminder error: {err}");
            (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response()
        }
    }
}

async fn cancel_reminder(
    State(state): State<AppState>,
    Path((telegram_user_id, reminder_id)): Path<(i64, i64)>,
) -> Response {
    let user_id = match resolve_user_id(&state.pool, telegram_user_id).await {
        Ok(id) => id,
        Err(resp) => return resp,
    };

    match crate::productivity::reminder_service::cancel_reminder(&state.pool, reminder_id, user_id)
        .await
    {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (StatusCode::NOT_FOUND, "Reminder not found").into_response(),
        Err(err) => {
            tracing::error!("cancel_reminder error: {err}");
            (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response()
        }
    }
}

// ── Feature Proposal handlers ──────────────────────────────────────────────────

async fn list_proposals_api(
    State(state): State<AppState>,
    Path(telegram_user_id): Path<i64>,
) -> Response {
    let user_id = match resolve_user_id(&state.pool, telegram_user_id).await {
        Ok(id) => id,
        Err(resp) => return resp,
    };

    let proposals = match crate::productivity::proposal_service::list_my_proposals(
        &state.pool,
        user_id,
    )
    .await
    {
        Ok(p) => p,
        Err(err) => {
            tracing::error!("list_proposals_api error: {err}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };

    Json(proposals).into_response()
}

#[derive(Deserialize)]
struct CreateProposalBody {
    telegram_user_id: i64,
    title: String,
    description: Option<String>,
}

async fn create_proposal_api(
    State(state): State<AppState>,
    Json(body): Json<CreateProposalBody>,
) -> Response {
    let user_id = match resolve_user_id(&state.pool, body.telegram_user_id).await {
        Ok(id) => id,
        Err(resp) => return resp,
    };

    match crate::productivity::proposal_service::create_proposal(
        &state.pool,
        user_id,
        body.title,
        body.description,
    )
    .await
    {
        Ok(proposal) => (StatusCode::CREATED, Json(proposal)).into_response(),
        Err(err) => {
            tracing::error!("create_proposal_api error: {err}");
            (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response()
        }
    }
}

async fn delete_proposal_api(
    State(state): State<AppState>,
    Path((telegram_user_id, proposal_id)): Path<(i64, i64)>,
) -> Response {
    let user = match resolve_user_id(&state.pool, telegram_user_id).await {
        Ok(id) => match db::users::find_by_id(&state.pool, id).await {
            Ok(Some(u)) => u,
            Ok(None) => {
                return (StatusCode::NOT_FOUND, "User not found").into_response();
            }
            Err(err) => {
                tracing::error!("delete_proposal_api user lookup error: {err}");
                return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
            }
        },
        Err(resp) => return resp,
    };

    let is_admin = user.role == Role::Admin;

    match crate::productivity::proposal_service::delete_proposal(
        &state.pool,
        proposal_id,
        user.id,
        is_admin,
    )
    .await
    {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (StatusCode::NOT_FOUND, "Proposal not found").into_response(),
        Err(err) => {
            tracing::error!("delete_proposal_api error: {err}");
            (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response()
        }
    }
}

// ── Nutrition handlers ────────────────────────────────────────────────────────

fn parse_api_date(raw: Option<&str>, timezone: &str) -> Result<chrono::NaiveDate, Response> {
    crate::bot::nutrition_table::parse_date_arg(raw.unwrap_or(""), timezone, chrono::Utc::now())
        .map_err(|_| {
            (
                StatusCode::BAD_REQUEST,
                "Invalid date. Use today, yesterday, or YYYY-MM-DD.",
            )
                .into_response()
        })
}

#[derive(Deserialize)]
struct NutritionDateQuery {
    date: Option<String>,
}

#[derive(Deserialize)]
struct CreateFoodBody {
    telegram_user_id: i64,
    name: String,
    brand: Option<String>,
    barcode: Option<String>,
    notes: Option<String>,
    kcal_per_100: Option<f64>,
    protein_g_per_100: Option<f64>,
    carbs_g_per_100: Option<f64>,
    fat_g_per_100: Option<f64>,
    kcal: Option<f64>,
    protein_g: Option<f64>,
    carbs_g: Option<f64>,
    fat_g: Option<f64>,
    serving_qty: Option<f64>,
    serving_unit: Option<String>,
    serving_label: Option<String>,
    source: Option<String>,
}

fn per_100_api(
    per_100: Option<f64>,
    serving: Option<f64>,
    serving_qty: f64,
    field: &str,
) -> Result<f64, Response> {
    if let Some(v) = per_100 {
        return Ok(v);
    }
    let Some(v) = serving else {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("{field}_per_100 or {field} (per serving) is required"),
        )
            .into_response());
    };
    db::foods::per_100_from_serving(v, serving_qty)
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()).into_response())
}

async fn maybe_embed_catalog_food(state: &AppState, food: &db::foods::Food) {
    crate::ai::food_selection::maybe_embed_food(
        &state.pool,
        state.gemini_api_token.as_deref(),
        &state.gemini_embedding_model,
        food,
    )
    .await;
}

async fn list_foods(State(state): State<AppState>, Path(telegram_user_id): Path<i64>) -> Response {
    let user_id = match resolve_user_id(&state.pool, telegram_user_id).await {
        Ok(id) => id,
        Err(resp) => return resp,
    };
    match db::foods::list_for_user(&state.pool, user_id).await {
        Ok(foods) => Json(foods).into_response(),
        Err(err) => {
            tracing::error!("list_foods error: {err}");
            (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response()
        }
    }
}

async fn create_food(State(state): State<AppState>, Json(body): Json<CreateFoodBody>) -> Response {
    let user_id = match resolve_user_id(&state.pool, body.telegram_user_id).await {
        Ok(id) => id,
        Err(resp) => return resp,
    };
    let serving_qty = body.serving_qty.unwrap_or(100.0);
    let serving_unit =
        match db::foods::QuantityUnit::parse(body.serving_unit.as_deref().unwrap_or("g")) {
            Ok(u) => u,
            Err(e) => return (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
        };
    let kcal_per_100 = match per_100_api(body.kcal_per_100, body.kcal, serving_qty, "kcal") {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let protein_g_per_100 = match per_100_api(
        body.protein_g_per_100,
        body.protein_g,
        serving_qty,
        "protein_g",
    ) {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let carbs_g_per_100 =
        match per_100_api(body.carbs_g_per_100, body.carbs_g, serving_qty, "carbs_g") {
            Ok(v) => v,
            Err(resp) => return resp,
        };
    let fat_g_per_100 = match per_100_api(body.fat_g_per_100, body.fat_g, serving_qty, "fat_g") {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let source = match body.source.as_deref() {
        Some("photo") => db::foods::FoodSource::Photo,
        Some("api") | None => db::foods::FoodSource::Api,
        Some("manual") => db::foods::FoodSource::Manual,
        Some(other) => {
            return (StatusCode::BAD_REQUEST, format!("unknown source: {other}")).into_response();
        }
    };

    match db::foods::insert(
        &state.pool,
        db::foods::NewFood {
            user_id,
            name: body.name,
            brand: body.brand,
            barcode: body.barcode,
            notes: body.notes,
            kcal_per_100,
            protein_g_per_100,
            carbs_g_per_100,
            fat_g_per_100,
            serving_qty,
            serving_unit,
            serving_label: body.serving_label,
            source,
        },
    )
    .await
    {
        Ok(food) => {
            maybe_embed_catalog_food(&state, &food).await;
            (StatusCode::CREATED, Json(food)).into_response()
        }
        Err(err) => {
            tracing::error!("create_food error: {err}");
            (StatusCode::BAD_REQUEST, err.to_string()).into_response()
        }
    }
}

#[derive(Deserialize)]
struct PatchFoodBody {
    name: Option<String>,
    brand: Option<String>,
    barcode: Option<String>,
    notes: Option<String>,
    kcal_per_100: Option<f64>,
    protein_g_per_100: Option<f64>,
    carbs_g_per_100: Option<f64>,
    fat_g_per_100: Option<f64>,
    serving_qty: Option<f64>,
    serving_unit: Option<String>,
    serving_label: Option<String>,
}

async fn update_food(
    State(state): State<AppState>,
    Path((telegram_user_id, food_id)): Path<(i64, i64)>,
    Json(body): Json<PatchFoodBody>,
) -> Response {
    let user_id = match resolve_user_id(&state.pool, telegram_user_id).await {
        Ok(id) => id,
        Err(resp) => return resp,
    };
    let serving_unit = match body.serving_unit.as_deref() {
        Some(s) => match db::foods::QuantityUnit::parse(s) {
            Ok(u) => Some(u),
            Err(e) => return (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
        },
        None => None,
    };
    let patch = db::foods::FoodUpdate {
        name: body.name,
        brand: body.brand.map(Some),
        barcode: body.barcode.map(Some),
        notes: body.notes.map(Some),
        kcal_per_100: body.kcal_per_100,
        protein_g_per_100: body.protein_g_per_100,
        carbs_g_per_100: body.carbs_g_per_100,
        fat_g_per_100: body.fat_g_per_100,
        serving_qty: body.serving_qty,
        serving_unit,
        serving_label: body.serving_label.map(Some),
        source: None,
    };
    match db::foods::update(&state.pool, user_id, food_id, patch).await {
        Ok(food) => {
            maybe_embed_catalog_food(&state, &food).await;
            Json(food).into_response()
        }
        Err(err) => {
            let msg = err.to_string();
            if msg.contains("not found") {
                (StatusCode::NOT_FOUND, msg).into_response()
            } else {
                (StatusCode::BAD_REQUEST, msg).into_response()
            }
        }
    }
}

async fn delete_food(
    State(state): State<AppState>,
    Path((telegram_user_id, food_id)): Path<(i64, i64)>,
) -> Response {
    let user_id = match resolve_user_id(&state.pool, telegram_user_id).await {
        Ok(id) => id,
        Err(resp) => return resp,
    };
    match db::foods::delete(&state.pool, user_id, food_id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (StatusCode::NOT_FOUND, "Food not found").into_response(),
        Err(err) => {
            tracing::error!("delete_food error: {err}");
            (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response()
        }
    }
}

#[derive(Deserialize)]
struct MealItemBody {
    food_id: Option<i64>,
    name: Option<String>,
    quantity: f64,
    unit: String,
    description: Option<String>,
    kcal: Option<f64>,
    protein_g: Option<f64>,
    carbs_g: Option<f64>,
    fat_g: Option<f64>,
}

#[derive(Deserialize)]
struct CreateMealBody {
    telegram_user_id: i64,
    kind: String,
    label: Option<String>,
    eaten_at: Option<String>,
    notes: Option<String>,
    items: Vec<MealItemBody>,
}

async fn resolve_api_item(
    pool: &crate::db::Db,
    user_id: i64,
    item: &MealItemBody,
) -> Result<db::meals::NewMealItem, Response> {
    let unit = db::foods::QuantityUnit::parse(&item.unit)
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()).into_response())?;
    if let Some(food_id) = item.food_id {
        let food = db::foods::get_for_user(pool, user_id, food_id)
            .await
            .map_err(|e| {
                tracing::error!("resolve_api_item food: {e}");
                (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response()
            })?
            .ok_or_else(|| {
                (
                    StatusCode::NOT_FOUND,
                    format!("food_id {food_id} not found"),
                )
                    .into_response()
            })?;
        return db::meals::item_from_food(&food, item.quantity, unit, item.description.clone())
            .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()).into_response());
    }
    if let Some(name) = item
        .name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let hits = db::foods::search_by_name(pool, user_id, name)
            .await
            .map_err(|e| {
                tracing::error!("resolve_api_item search: {e}");
                (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response()
            })?;
        let exact: Vec<_> = hits
            .iter()
            .filter(|f| f.name.eq_ignore_ascii_case(name))
            .cloned()
            .collect();
        let chosen = if exact.len() == 1 {
            Some(exact.into_iter().next().unwrap())
        } else if hits.len() == 1 {
            hits.into_iter().next()
        } else {
            None
        };
        if let Some(food) = chosen {
            return db::meals::item_from_food(&food, item.quantity, unit, item.description.clone())
                .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()).into_response());
        }
    }
    let macros = match (item.kcal, item.protein_g, item.carbs_g, item.fat_g) {
        (Some(kcal), Some(protein_g), Some(carbs_g), Some(fat_g)) => db::foods::Macros {
            kcal,
            protein_g,
            carbs_g,
            fat_g,
        },
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                "Item needs food_id/name from the catalog, or kcal/protein_g/carbs_g/fat_g",
            )
                .into_response());
        }
    };
    let desc = item
        .description
        .clone()
        .or_else(|| item.name.clone())
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                "description or name is required for estimated items",
            )
                .into_response()
        })?;
    db::meals::estimated_item(
        desc,
        item.quantity,
        unit,
        macros,
        db::meals::ItemSource::Estimated,
    )
    .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()).into_response())
}

async fn day_nutrition_for(
    state: &AppState,
    telegram_user_id: i64,
    date: Option<&str>,
) -> Response {
    let user = match db::users::find_by_telegram_id(&state.pool, telegram_user_id).await {
        Ok(Some(u)) => u,
        Ok(None) => return (StatusCode::NOT_FOUND, "User not found").into_response(),
        Err(err) => {
            tracing::error!("day_nutrition user: {err}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };
    let date = match parse_api_date(date, &user.timezone) {
        Ok(d) => d,
        Err(resp) => return resp,
    };
    match db::meals::day_for_user(&state.pool, user.id, &user.timezone, date).await {
        Ok(day) => Json(day).into_response(),
        Err(err) => {
            tracing::error!("day_nutrition error: {err}");
            (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response()
        }
    }
}

async fn list_meals(
    State(state): State<AppState>,
    Path(telegram_user_id): Path<i64>,
    Query(query): Query<NutritionDateQuery>,
) -> Response {
    day_nutrition_for(&state, telegram_user_id, query.date.as_deref()).await
}

async fn create_meal(State(state): State<AppState>, Json(body): Json<CreateMealBody>) -> Response {
    let user = match db::users::find_by_telegram_id(&state.pool, body.telegram_user_id).await {
        Ok(Some(u)) => u,
        Ok(None) => return (StatusCode::NOT_FOUND, "User not found").into_response(),
        Err(err) => {
            tracing::error!("create_meal user: {err}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };
    let kind = match db::meals::MealKind::parse(&body.kind) {
        Ok(k) => k,
        Err(e) => return (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    };
    if body.items.is_empty() {
        return (StatusCode::BAD_REQUEST, "items must not be empty").into_response();
    }
    let mut items = Vec::new();
    for item in &body.items {
        match resolve_api_item(&state.pool, user.id, item).await {
            Ok(i) => items.push(i),
            Err(resp) => return resp,
        }
    }
    let tz: chrono_tz::Tz = user.timezone.parse().unwrap_or(chrono_tz::UTC);
    let eaten_at = match body.eaten_at.as_deref() {
        Some(s) => match crate::datetime::parse_flexible_datetime(s, &tz, chrono::Utc::now()) {
            Some(d) => d,
            None => return (StatusCode::BAD_REQUEST, "Invalid eaten_at").into_response(),
        },
        None => chrono::Utc::now(),
    };
    match db::meals::insert(
        &state.pool,
        db::meals::NewMeal {
            user_id: user.id,
            kind,
            label: body.label,
            eaten_at,
            notes: body.notes,
            items,
        },
    )
    .await
    {
        Ok(meal) => (StatusCode::CREATED, Json(meal)).into_response(),
        Err(err) => {
            tracing::error!("create_meal error: {err}");
            (StatusCode::BAD_REQUEST, err.to_string()).into_response()
        }
    }
}

async fn delete_meal(
    State(state): State<AppState>,
    Path((telegram_user_id, meal_id)): Path<(i64, i64)>,
) -> Response {
    let user_id = match resolve_user_id(&state.pool, telegram_user_id).await {
        Ok(id) => id,
        Err(resp) => return resp,
    };
    match db::meals::delete(&state.pool, user_id, meal_id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (StatusCode::NOT_FOUND, "Meal not found").into_response(),
        Err(err) => {
            tracing::error!("delete_meal error: {err}");
            (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response()
        }
    }
}

async fn get_day_nutrition(
    State(state): State<AppState>,
    Path(telegram_user_id): Path<i64>,
    Query(query): Query<NutritionDateQuery>,
) -> Response {
    day_nutrition_for(&state, telegram_user_id, query.date.as_deref()).await
}

// ── Health ────────────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
}

async fn health() -> impl IntoResponse {
    Json(HealthResponse { status: "ok" })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::tests::test_db;
    use crate::db::users::{Role, insert as insert_user};
    use std::path::PathBuf;

    const API_KEY: &str = "test-bot-token";
    const TG_USER: i64 = 424242;

    async fn spawn_app() -> (String, PathBuf) {
        let (base, path, _) = spawn_app_with_embed(None).await;
        (base, path)
    }

    async fn spawn_app_with_embed(
        gemini_api_token: Option<&str>,
    ) -> (String, PathBuf, crate::db::Db) {
        let (pool, path) = test_db().await;
        insert_user(
            &pool,
            TG_USER,
            Role::Sportive,
            Some("tester".into()),
            None,
            None,
        )
        .await
        .unwrap();
        let state = AppState {
            pool: pool.clone(),
            logs: crate::logs::SharedLogBuffer::new(10),
            admin_api_key: API_KEY.to_string(),
            gemini_api_token: gemini_api_token.map(str::to_string),
            gemini_embedding_model: "test-embedding-model".to_string(),
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind test listener");
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, router(state)).await.unwrap();
        });
        let base = format!("http://{addr}");
        let client = reqwest::Client::new();
        for _ in 0..50 {
            if client.get(format!("{base}/health")).send().await.is_ok() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        (base, path, pool)
    }

    async fn spawn_app_with_logs() -> (String, PathBuf, crate::logs::SharedLogBuffer) {
        let (pool, path) = test_db().await;
        insert_user(
            &pool,
            TG_USER,
            Role::Sportive,
            Some("tester".into()),
            None,
            None,
        )
        .await
        .unwrap();
        let logs = crate::logs::SharedLogBuffer::new(20);
        let state = AppState {
            pool,
            logs: logs.clone(),
            admin_api_key: API_KEY.to_string(),
            gemini_api_token: None,
            gemini_embedding_model: "test-embedding-model".to_string(),
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind test listener");
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, router(state)).await.unwrap();
        });
        let base = format!("http://{addr}");
        let client = reqwest::Client::new();
        for _ in 0..50 {
            if client.get(format!("{base}/health")).send().await.is_ok() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        (base, path, logs)
    }

    fn client() -> reqwest::Client {
        reqwest::Client::new()
    }

    #[tokio::test]
    async fn nutrition_routes_reject_missing_and_wrong_api_key() {
        let (base, path) = spawn_app().await;
        let client = client();
        let food_body = serde_json::json!({
            "telegram_user_id": TG_USER,
            "name": "Whey",
            "kcal_per_100": 370.0,
            "protein_g_per_100": 80.0,
            "carbs_g_per_100": 6.0,
            "fat_g_per_100": 4.0
        });
        let meal_body = serde_json::json!({
            "telegram_user_id": TG_USER,
            "kind": "lunch",
            "items": [{
                "name": "Whey",
                "quantity": 30.0,
                "unit": "g",
                "kcal": 111.0,
                "protein_g": 24.0,
                "carbs_g": 1.8,
                "fat_g": 1.2
            }]
        });
        let patch_body = serde_json::json!({ "name": "stolen" });

        let unauthenticated = [
            client.get(format!("{base}/api/foods/{TG_USER}")).build(),
            client
                .post(format!("{base}/api/foods"))
                .json(&food_body)
                .build(),
            client
                .patch(format!("{base}/api/foods/{TG_USER}/1"))
                .json(&patch_body)
                .build(),
            client
                .delete(format!("{base}/api/foods/{TG_USER}/1"))
                .build(),
            client.get(format!("{base}/api/meals/{TG_USER}")).build(),
            client
                .post(format!("{base}/api/meals"))
                .json(&meal_body)
                .build(),
            client
                .delete(format!("{base}/api/meals/{TG_USER}/1"))
                .build(),
            client
                .get(format!("{base}/api/nutrition/{TG_USER}"))
                .build(),
        ];

        for req in unauthenticated {
            let resp = client.execute(req.unwrap()).await.unwrap();
            assert_eq!(
                resp.status(),
                reqwest::StatusCode::UNAUTHORIZED,
                "expected 401 for {}",
                resp.url()
            );
        }

        let wrong_key = client
            .get(format!("{base}/api/foods/{TG_USER}"))
            .header("X-API-Key", "wrong-token")
            .send()
            .await
            .unwrap();
        assert_eq!(wrong_key.status(), reqwest::StatusCode::UNAUTHORIZED);

        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn nutrition_routes_succeed_with_valid_api_key() {
        let (base, path) = spawn_app().await;
        let client = client();

        let created = client
            .post(format!("{base}/api/foods"))
            .header("X-API-Key", API_KEY)
            .json(&serde_json::json!({
                "telegram_user_id": TG_USER,
                "name": "Whey Isolate",
                "kcal_per_100": 370.0,
                "protein_g_per_100": 80.0,
                "carbs_g_per_100": 6.0,
                "fat_g_per_100": 4.0,
                "serving_qty": 30.0,
                "serving_unit": "g"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(created.status(), reqwest::StatusCode::CREATED);
        let food: serde_json::Value = created.json().await.unwrap();
        let food_id = food["id"].as_i64().expect("food id");

        let listed = client
            .get(format!("{base}/api/foods/{TG_USER}"))
            .header("X-API-Key", API_KEY)
            .send()
            .await
            .unwrap();
        assert_eq!(listed.status(), reqwest::StatusCode::OK);
        let foods: Vec<serde_json::Value> = listed.json().await.unwrap();
        assert_eq!(foods.len(), 1);
        assert_eq!(foods[0]["name"], "Whey Isolate");

        let meal = client
            .post(format!("{base}/api/meals"))
            .header("X-API-Key", API_KEY)
            .json(&serde_json::json!({
                "telegram_user_id": TG_USER,
                "kind": "breakfast",
                "items": [{
                    "food_id": food_id,
                    "quantity": 30.0,
                    "unit": "g"
                }]
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(meal.status(), reqwest::StatusCode::CREATED);
        let meal: serde_json::Value = meal.json().await.unwrap();
        let meal_id = meal["id"].as_i64().expect("meal id");

        let day = client
            .get(format!("{base}/api/nutrition/{TG_USER}"))
            .header("X-API-Key", API_KEY)
            .send()
            .await
            .unwrap();
        assert_eq!(day.status(), reqwest::StatusCode::OK);
        let day: serde_json::Value = day.json().await.unwrap();
        assert_eq!(day["meals"].as_array().map(Vec::len), Some(1));

        let deleted = client
            .delete(format!("{base}/api/meals/{TG_USER}/{meal_id}"))
            .header("X-API-Key", API_KEY)
            .send()
            .await
            .unwrap();
        assert_eq!(deleted.status(), reqwest::StatusCode::NO_CONTENT);

        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn create_and_update_food_persist_catalog_embeddings() {
        use crate::ai::food_selection::TEST_EMBED_API_KEY;

        let (base, path, pool) = spawn_app_with_embed(Some(TEST_EMBED_API_KEY)).await;
        let client = client();
        let user = crate::db::users::find_by_telegram_id(&pool, TG_USER)
            .await
            .unwrap()
            .unwrap();

        let created = client
            .post(format!("{base}/api/foods"))
            .header("X-API-Key", API_KEY)
            .json(&serde_json::json!({
                "telegram_user_id": TG_USER,
                "name": "Whey Isolate",
                "brand": "MyBrand",
                "kcal_per_100": 370.0,
                "protein_g_per_100": 80.0,
                "carbs_g_per_100": 6.0,
                "fat_g_per_100": 4.0
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(created.status(), reqwest::StatusCode::CREATED);
        let food: serde_json::Value = created.json().await.unwrap();
        let food_id = food["id"].as_i64().expect("food id");

        let embeddings = crate::db::foods::list_embeddings(&pool, user.id)
            .await
            .unwrap();
        assert_eq!(embeddings.len(), 1);
        assert_eq!(embeddings[0].food_id, food_id);
        let embed_text: String =
            sqlx::query_scalar("SELECT embed_text FROM food_embeddings WHERE food_id = ?")
                .bind(food_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(embed_text, "MyBrand Whey Isolate");

        let patched = client
            .patch(format!("{base}/api/foods/{TG_USER}/{food_id}"))
            .header("X-API-Key", API_KEY)
            .json(&serde_json::json!({ "name": "Whey Concentrate" }))
            .send()
            .await
            .unwrap();
        assert_eq!(patched.status(), reqwest::StatusCode::OK);

        let embeddings = crate::db::foods::list_embeddings(&pool, user.id)
            .await
            .unwrap();
        assert_eq!(embeddings.len(), 1);
        let embed_text: String =
            sqlx::query_scalar("SELECT embed_text FROM food_embeddings WHERE food_id = ?")
                .bind(food_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(embed_text, "MyBrand Whey Concentrate");

        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn create_food_skips_embedding_without_gemini_token() {
        let (base, path, pool) = spawn_app_with_embed(None).await;
        let client = client();
        let user = crate::db::users::find_by_telegram_id(&pool, TG_USER)
            .await
            .unwrap()
            .unwrap();

        let created = client
            .post(format!("{base}/api/foods"))
            .header("X-API-Key", API_KEY)
            .json(&serde_json::json!({
                "telegram_user_id": TG_USER,
                "name": "Oats",
                "kcal_per_100": 370.0,
                "protein_g_per_100": 13.0,
                "carbs_g_per_100": 60.0,
                "fat_g_per_100": 7.0
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(created.status(), reqwest::StatusCode::CREATED);
        let embeddings = crate::db::foods::list_embeddings(&pool, user.id)
            .await
            .unwrap();
        assert!(embeddings.is_empty());

        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn logs_require_api_key_and_honor_filters() {
        let (base, path, logs) = spawn_app_with_logs().await;
        let client = client();

        let denied = client.get(format!("{base}/api/logs")).send().await.unwrap();
        assert_eq!(denied.status(), reqwest::StatusCode::UNAUTHORIZED);

        logs.push(crate::logs::LogEntry {
            timestamp: "2026-08-25T00:00:00.000Z".into(),
            level: "INFO".into(),
            target: "gregobot::bot".into(),
            span_name: None,
            span_id: None,
            fields: Vec::new(),
            message: "started".into(),
        });
        logs.push(crate::logs::LogEntry {
            timestamp: "2026-08-25T00:00:01.000Z".into(),
            level: "ERROR".into(),
            target: "gregobot::bot".into(),
            span_name: None,
            span_id: None,
            fields: Vec::new(),
            message: "webhook 401".into(),
        });

        let filtered = client
            .get(format!("{base}/api/logs?level=ERROR&format=text"))
            .header("X-API-Key", API_KEY)
            .send()
            .await
            .unwrap();
        assert_eq!(filtered.status(), reqwest::StatusCode::OK);
        let body = filtered.text().await.unwrap();
        assert!(body.contains("webhook 401"));
        assert!(!body.contains("started"));

        let json = client
            .get(format!("{base}/api/logs?q=webhook"))
            .header("X-API-Key", API_KEY)
            .send()
            .await
            .unwrap();
        assert_eq!(json.status(), reqwest::StatusCode::OK);
        let rows: Vec<serde_json::Value> = json.json().await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["message"], "webhook 401");

        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn all_api_routes_reject_missing_api_key() {
        let (base, path) = spawn_app().await;
        let client = client();

        let unauthenticated = [
            client.get(format!("{base}/api/weight/{TG_USER}")).build(),
            client
                .get(format!("{base}/api/weight/{TG_USER}/latest"))
                .build(),
            client
                .post(format!("{base}/api/weight"))
                .json(&serde_json::json!({
                    "telegram_user_id": TG_USER,
                    "weight_kg": 80.0
                }))
                .build(),
            client
                .get(format!("{base}/api/training/{TG_USER}"))
                .build(),
            client
                .post(format!("{base}/api/training"))
                .json(&serde_json::json!({
                    "telegram_user_id": TG_USER,
                    "description": "run"
                }))
                .build(),
            client.get(format!("{base}/api/users")).build(),
            client
                .post(format!("{base}/api/users"))
                .json(&serde_json::json!({
                    "telegram_user_id": 999,
                    "role": "sportive"
                }))
                .build(),
            client.delete(format!("{base}/api/users/{TG_USER}")).build(),
            client.get(format!("{base}/api/tasks/{TG_USER}")).build(),
            client
                .get(format!("{base}/api/reminders/{TG_USER}"))
                .build(),
            client
                .get(format!("{base}/api/proposals/{TG_USER}"))
                .build(),
            client.get(format!("{base}/api/ai/pricing")).build(),
        ];

        for req in unauthenticated {
            let resp = client.execute(req.unwrap()).await.unwrap();
            assert_eq!(
                resp.status(),
                reqwest::StatusCode::UNAUTHORIZED,
                "expected 401 for {}",
                resp.url()
            );
        }

        let health = client.get(format!("{base}/health")).send().await.unwrap();
        assert_eq!(health.status(), reqwest::StatusCode::OK);

        let _ = std::fs::remove_file(&path);
    }
}
