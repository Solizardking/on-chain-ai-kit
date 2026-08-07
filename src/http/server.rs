use actix_cors::Cors;
use actix_files as fs;
use actix_web::dev::{Service, ServiceRequest, ServiceResponse, Transform};
use actix_web::middleware::Logger;
use actix_web::{web, App, Error, HttpResponse, HttpServer};
use futures_util::future::LocalBoxFuture;
use std::future::{ready, Ready};
use std::path::PathBuf;
use std::rc::Rc;

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

/// actix-files returns 400 for path segments starting with `.` (e.g. `/.env` scanners).
/// Map those to clean 404 so probes don't look like a misconfigured server.
struct HiddenPathNotFound;

impl<S, B> Transform<S, ServiceRequest> for HiddenPathNotFound
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Transform = HiddenPathNotFoundMiddleware<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(HiddenPathNotFoundMiddleware {
            service: Rc::new(service),
        }))
    }
}

struct HiddenPathNotFoundMiddleware<S> {
    service: Rc<S>,
}

impl<S, B> Service<ServiceRequest> for HiddenPathNotFoundMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    actix_web::dev::forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let path = req.path().to_string();
        // actix-files 400s on segments starting with '.' — answer 404 instead.
        let has_hidden = path
            .split('/')
            .any(|seg| seg.starts_with('.') && seg != "." && seg != "..");
        let svc = self.service.clone();
        Box::pin(async move {
            if has_hidden {
                return Err(actix_web::error::ErrorNotFound("not found"));
            }
            svc.call(req).await
        })
    }
}

async fn not_found() -> HttpResponse {
    HttpResponse::NotFound()
        .content_type("text/plain; charset=utf-8")
        .body("not found")
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
            .wrap(HiddenPathNotFound)
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
                    .prefer_utf8(true)
                    .default_handler(web::to(not_found)),
            )
            .default_service(web::to(not_found))
    })
    .bind("0.0.0.0:6969")?
    .run()
    .await
}
