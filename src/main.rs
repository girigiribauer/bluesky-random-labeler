use omikuji::config::config;
use omikuji::db::init_db;
use omikuji::api::router;
use omikuji::state::AppState;
use omikuji::crypto::create_keypair;
use omikuji::{poller, scheduler};
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio_cron_scheduler::{Job, JobScheduler};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let conf = config();
    tracing::info!(port = conf.port, "Starting Bluesky Random Labeler");

    let pool = init_db(&conf.db_path).await?;

    let keypair = Arc::new(create_keypair(&conf.signing_key_hex)?);
    let (tx, _rx) = tokio::sync::broadcast::channel(10000);

    let pool_clone = pool.clone();
    let keypair_clone = keypair.clone();
    let tx_for_poller = tx.clone();
    tokio::spawn(async move {
        if let Err(e) = poller::start_polling(pool_clone, keypair_clone, tx_for_poller).await {
            tracing::error!(error = ?e, "Poller failed");
        }
    });

    // Run batch on startup
    let startup_pool = pool.clone();
    let startup_tx = tx.clone();
    tokio::spawn(async move {
        if std::env::var("RUN_MIGRATION").is_ok() {
            tracing::info!("RUN_MIGRATION env var detected. Starting migration batch...");
            if let Err(e) = scheduler::run_migration(startup_pool, startup_tx).await {
                tracing::error!(error = ?e, "Migration batch failed");
            }
        } else {
            // Normal startup batch
            if let Err(e) = scheduler::run_optimized_batch(startup_pool, startup_tx).await {
                tracing::error!(error = ?e, "Startup batch failed");
            }
        }
    });



    let sched_pool = pool.clone();
    let sched_tx = tx.clone();
    let sched = JobScheduler::new().await?;

    sched.add(
        Job::new_async("0 0 15 * * *", move |_uuid, _l| {
            let p = sched_pool.clone();
            let tx = sched_tx.clone();
            Box::pin(async move {
                if let Err(e) = scheduler::run_optimized_batch(p, tx).await {
                    tracing::error!(error = ?e, "Scheduler batch failed");
                }
            })
        })?
    ).await?;
    sched.start().await?;

    let state = AppState {
        pool,
        keypair,
        tx,
    };

    let app = router(state);
    let addr = format!("0.0.0.0:{}", conf.port);
    let listener = TcpListener::bind(&addr).await?;
    tracing::info!(address = %addr, "Server bound");

    axum::serve(listener, app).await?;

    Ok(())
}
