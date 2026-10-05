mod ai;
mod api;
mod bot;
mod config;
mod datetime;
mod db;
mod embeddings;
mod error_webhook;
mod i18n;
mod log_persist;
mod logs;
mod media;
mod productivity;

use anyhow::Result;
use tracing::{error, info, warn};
use tracing_subscriber::Registry;
use tracing_subscriber::fmt::Layer as FmtLayer;
use tracing_subscriber::layer::SubscriberExt;

use crate::ai::context::AiContext;
use crate::api::AppState;
use crate::logs::{SharedLogBuffer, StructuredLogLayer};
use tokio::sync::mpsc;

struct LogPersistChannels {
    app_rx: mpsc::Receiver<crate::logs::LogEntry>,
    ai_tx: mpsc::Sender<crate::ai::log::AiTrace>,
    ai_rx: mpsc::Receiver<crate::ai::log::AiTrace>,
}

fn main() -> Result<()> {
    // Load .env file if present (silently ignore if missing)
    let _ = dotenvy::dotenv();

    let log_max = crate::logs::max_entries_from_env();
    let log_buffer = SharedLogBuffer::new(log_max);
    let (app_persist_tx, app_persist_rx) = mpsc::channel(8192);
    let (ai_persist_tx, ai_persist_rx) = mpsc::channel(256);
    log_buffer.attach_persist(app_persist_tx);
    let error_webhook = error_webhook::ErrorWebhookSecrets::from_env();
    let error_webhook_rx = if error_webhook.is_some() {
        let (tx, rx) = tokio::sync::mpsc::channel(error_webhook::QUEUE_CAPACITY);
        init_tracing(log_buffer.clone(), Some(tx))?;
        Some(rx)
    } else {
        init_tracing(log_buffer.clone(), None)?;
        None
    };

    let config = config::Config::from_env()
        .inspect_err(|e| error!("Failed to load configuration: {}", e))?;
    tracing::info!("Config loaded, port={}", config.port);
    info!("Starting gregobot on port {}", config.port);

    // Use a custom runtime with 8 MiB worker thread stacks.
    // The dptree handler chain creates deep call stacks that debug builds
    // don't inline away, exceeding the default 2 MiB tokio worker stack.
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(8 * 1024 * 1024)
        .build()?;

    tracing::info!("Entering rt.block_on");
    let result = rt.block_on(async_main(
        config,
        log_buffer,
        error_webhook,
        error_webhook_rx,
        LogPersistChannels {
            app_rx: app_persist_rx,
            ai_tx: ai_persist_tx,
            ai_rx: ai_persist_rx,
        },
    ));
    tracing::info!(
        "[DIAG] rt.block_on returned: {:?}",
        result.as_ref().map(|_| "Ok")
    );
    result
}

fn init_tracing(
    log_buffer: SharedLogBuffer,
    error_tx: Option<tokio::sync::mpsc::Sender<error_webhook::ErrorEvent>>,
) -> Result<()> {
    // Initialise structured logging (controlled via RUST_LOG env var).
    // stdout receives human-readable logs; the StructuredLogLayer stores
    // structured entries (level, target, span, fields) in the ring buffer
    // and queues them for SQLite persist (loaded again after restart).
    // When ERROR_CURSOR_WEBHOOK_URL and ERROR_CURSOR_WEBHOOK_BEARER are both set,
    // ERROR events also enqueue a Cursor Automations webhook payload (consumed
    // after the runtime starts).
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    let registry = Registry::default()
        .with(env_filter)
        .with(FmtLayer::new().with_writer(std::io::stdout))
        .with(StructuredLogLayer::new(log_buffer));

    if let Some(tx) = error_tx {
        tracing::subscriber::set_global_default(
            registry.with(error_webhook::ErrorWebhookLayer::new(tx)),
        )?;
    } else {
        tracing::subscriber::set_global_default(registry)?;
    }
    Ok(())
}

async fn async_main(
    config: config::Config,
    log_buffer: SharedLogBuffer,
    error_webhook: Option<error_webhook::ErrorWebhookSecrets>,
    error_rx: Option<tokio::sync::mpsc::Receiver<error_webhook::ErrorEvent>>,
    persist: LogPersistChannels,
) -> Result<()> {
    match (error_webhook, error_rx) {
        (Some(secrets), Some(rx)) => {
            error_webhook::spawn_worker(rx, secrets.bearer, secrets.url.clone());
            info!(
                "ERROR logs will POST to Cursor Automations webhook ({})",
                secrets.url
            );
        }
        _ => {
            info!(
                "{} and {} must both be set — error automation webhook disabled",
                error_webhook::URL_ENV,
                error_webhook::BEARER_ENV
            );
        }
    }

    let ai_ctx = AiContext::from_config(&config);

    // Database: connect and run migrations
    let pool = db::init(&config.database_url)
        .await
        .inspect_err(|e| error!("Failed to initialize database: {}", e))?;
    tracing::info!("db::init done");
    info!("Database ready");
    let log_max = config.log_max_entries;
    let ai_max = config.ai_log_max_entries;
    info!(log_max, ai_max, "Log persist caps (app lines / AI traces)");

    match db::app_logs::load_latest(&pool, log_max).await {
        Ok(entries) => {
            let count = entries.len();
            log_buffer.prepend_persisted(entries);
            if count > 0 {
                info!(count, "Loaded persisted app logs");
            }
        }
        Err(e) => warn!("Failed to load persisted app logs: {e}"),
    }

    let ai_logs = crate::ai::log::global();
    ai_logs.set_max(ai_max);
    match db::ai_traces::load_latest(&pool, ai_max).await {
        Ok(traces) => {
            let count = traces.len();
            ai_logs.load_persisted(traces);
            if count > 0 {
                info!(count, "Loaded persisted AI traces");
            }
        }
        Err(e) => warn!("Failed to load persisted AI traces: {e}"),
    }
    ai_logs.attach_persist(persist.ai_tx);
    log_persist::spawn(pool.clone(), persist.app_rx, persist.ai_rx, log_max, ai_max);

    // Seed the admin user if ADMIN_TELEGRAM_USER_ID is configured
    if let Some(admin_id) = config.admin_telegram_user_id {
        db::users::seed_admin(&pool, admin_id).await?;
        tracing::info!("seed_admin done");
        info!("Admin user seeded (telegram_user_id={admin_id})");
    }

    // Pre-compute and cache embeddings for tool selection and exercise KNN.
    if let Some(ref api_key) = config.gemini_api_token {
        if let Err(e) =
            ai::tool_selection::ensure_embeddings(&pool, api_key, &config.gemini_embedding_model)
                .await
        {
            warn!("Failed to initialize tool embeddings: {e}");
        }
        if let Err(e) = ai::exercise_selection::ensure_embeddings(
            &pool,
            api_key,
            &config.gemini_embedding_model,
        )
        .await
        {
            warn!("Failed to initialize exercise embeddings: {e}");
        }
    }
    tracing::info!("Embeddings ready");

    // Build webhook listener + Axum router fragment + dispatcher future
    let (webhook_router, dispatcher_future) = bot::build(&config, pool.clone())
        .await
        .inspect_err(|e| error!("Failed to build bot/webhook: {}", e))?;
    tracing::info!(" bot::build done");

    // Clone pool for background tasks before it's moved into AppState
    let scheduler_pool = pool.clone();

    let state = AppState {
        pool,
        logs: log_buffer.clone(),
        admin_api_key: config.admin_api_key.clone(),
        gemini_api_token: config.gemini_api_token.clone(),
        gemini_embedding_model: config.gemini_embedding_model.clone(),
    };

    if std::env::var("SEED_AI_TRACES").ok().as_deref() == Some("1") {
        crate::ai::log::seed_preview_traces();
        info!("Seeded preview AI traces for /ai/logs");
    }

    // Combine webhook routes with admin API routes.
    let app = api::router(state).merge(webhook_router);

    let addr: std::net::SocketAddr = format!("0.0.0.0:{}", config.port).parse()?;
    info!("Listening on {addr}");

    let listener = tokio::net::TcpListener::bind(addr).await?;

    // Spawn the reminder scheduler background task
    bot::scheduler::spawn(
        scheduler_pool.clone(),
        config.bot_token.clone(),
        config.reminder_check_interval_secs,
    );

    // Spawn the proactive AI scheduler (if API key is configured)
    if ai_ctx.api_key().is_some() {
        bot::proactive::spawn(
            scheduler_pool.clone(),
            config.bot_token.clone(),
            ai_ctx.clone(),
        );
    }

    // Spawn the daily recap scheduler (if API key is configured)
    if ai_ctx.api_key().is_some() {
        bot::daily_recap::spawn(
            scheduler_pool.clone(),
            config.bot_token.clone(),
            ai_ctx.clone(),
            config.daily_recap_check_interval_secs,
        );
    }

    // Spawn the nudge scheduler (if API key is configured)
    if ai_ctx.api_key().is_some() {
        bot::nudge::spawn(
            scheduler_pool.clone(),
            config.bot_token.clone(),
            ai_ctx.clone(),
            config.nudge_check_interval_secs,
        );
    }

    // Opt-in end-of-month training graphs for sport groups
    bot::monthly_graph::spawn(scheduler_pool, config.bot_token.clone());

    // Run both the Axum server and Teloxide dispatcher concurrently
    tracing::info!("Entering tokio::select! (axum vs dispatcher)");
    tokio::select! {
        result = axum::serve(listener, app) => {
            tracing::info!("select! => axum::serve completed");
            match result {
                Ok(()) => info!("HTTP server shut down gracefully"),
                Err(e) => {
                    error!("HTTP server stopped with error: {:#}", e);
                    return Err(e.into());
                }
            }
        }
        _ = dispatcher_future => {
            tracing::info!(" select! => dispatcher_future completed");
            warn!("Bot dispatcher stopped — webhook listener may have disconnected. Check Telegram getWebhookInfo.");
        }
    }

    tracing::info!("async_main returning Ok(())");
    Ok(())
}
