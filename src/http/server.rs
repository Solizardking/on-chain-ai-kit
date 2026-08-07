use actix_cors::Cors;
use actix_files as fs;
use actix_web::middleware::Logger;
use actix_web::{web, App, HttpServer};
use std::path::PathBuf;

use super::agents::{
    agents_health, create_agent, get_agent_by_id, get_agent_catalog, mint_agent,
};
use super::routes::{auth, healthz, stream};
use super::state::AppState;

fn resolve_static_dir() -> PathBuf {
    // Prefer explicit deploy path (Docker/Fly), then crate root, then CWD.
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(custom) = std::env::var("CLAWD_FRONTEND_DIR").or_else(|_| std::env::var("KIT_FRONTEND_DIR"))
    {
        if !custom.trim().is_empty() {
            candidates.push(PathBuf::from(custom));
        }
    }
    candidates.push(PathBuf::from("/app/frontend"));
    candidates.push(PathBuf::from("/app/frontend/dist"));
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    candidates.push(manifest.join("frontend/dist"));
    candidates.push(manifest.join("frontend"));
    candidates.push(PathBuf::from("./frontend/dist"));
    candidates.push(PathBuf::from("./frontend"));
    for c in candidates {
        if c.is_dir() {
            return c;
        }
    }
    manifest.join("frontend")
}

pub async fn run_server(state: AppState) -> std::io::Result<()> {
    let state = web::Data::new(state);
    let static_dir = resolve_static_dir();
    eprintln!(
        "openclawd-kit: serving static frontend from {}",
        static_dir.display()
    );

    // Note: do NOT wrap the whole app in Compress — it buffers SSE on /stream.
    HttpServer::new(move || {
        let static_dir = static_dir.clone();
        App::new()
            .wrap(Logger::default())
            .wrap(Cors::permissive())
            .app_data(state.clone())
            // API first so they win over the catch-all Files service
            .service(healthz)
            .service(stream)
            .service(auth)
            .service(get_agent_catalog)
            .service(get_agent_by_id)
            .service(mint_agent)
            .service(create_agent)
            .service(agents_health)
            .service(
                fs::Files::new("/", static_dir)
                    .index_file("index.html")
                    .prefer_utf8(true),
            )
    })
    .bind("0.0.0.0:6969")?
    .run()
    .await
}
