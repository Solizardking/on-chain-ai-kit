use actix_cors::Cors;
use actix_files as fs;
use actix_web::middleware::{Compress, Logger};
use actix_web::{web, App, HttpServer};

use super::agents::{
    agents_health, create_agent, get_agent_by_id, get_agent_catalog, mint_agent,
};
use super::routes::{auth, healthz, stream};
use super::state::AppState;

pub async fn run_server(state: AppState) -> std::io::Result<()> {
    let state = web::Data::new(state);

    // Prefer built SPA if present, else plain frontend/
    let static_dir = if std::path::Path::new("./frontend/dist").is_dir() {
        "./frontend/dist"
    } else {
        "./frontend"
    };

    HttpServer::new(move || {
        App::new()
            .wrap(Logger::default())
            .wrap(Compress::default())
            .wrap(Cors::permissive())
            .app_data(state.clone())
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
                    .show_files_listing(),
            )
    })
    .bind("0.0.0.0:6969")?
    .run()
    .await
}
