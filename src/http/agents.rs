use crate::http::state::AppState;
use actix_web::{get, post, web, HttpResponse};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;

/// Agent definition from the agents catalog / lobster council
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AgentMeta {
    pub title: String,
    pub description: String,
    pub avatar: String,
    pub tags: Vec<String>,
    pub category: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AgentConfig {
    pub system_role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AgentDefinition {
    pub identifier: String,
    pub author: String,
    pub meta: AgentMeta,
    pub config: serde_json::Value,
    pub homepage: String,
    #[serde(default)]
    pub locale: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voice: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AgentCatalog {
    pub agents: Vec<CatalogEntry>,
    pub stats: CatalogStats,
    pub categories: Vec<CategoryDef>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CatalogEntry {
    pub identifier: String,
    pub title: String,
    pub description: String,
    pub avatar: String,
    pub tags: Vec<String>,
    pub category: String,
    pub author: String,
    pub on_chain: bool,
    pub mint_address: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CatalogStats {
    pub total: usize,
    pub on_chain: usize,
    pub by_category: std::collections::HashMap<String, usize>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CategoryDef {
    pub id: String,
    pub label: String,
    pub icon: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MintAgentRequest {
    pub identifier: String,
    pub wallet_address: String,
}

/// Resolve the lobster-council/ and agents directory paths
fn get_agents_dir() -> PathBuf {
    // First try: lobster-council/ in the project root
    let cwd = std::env::current_dir().unwrap_or_default();
    let lobster = cwd.join("lobster-council");
    if lobster.exists() {
        return lobster;
    }
    // Fallback: look relative to this binary
    PathBuf::from("lobster-council")
}

fn load_all_agent_definitions() -> Vec<AgentDefinition> {
    let agents_dir = get_agents_dir();
    let mut agents = Vec::new();

    if !agents_dir.exists() {
        return agents;
    }

    if let Ok(entries) = fs::read_dir(&agents_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map_or(false, |ext| ext == "json") {
                if let Ok(content) = fs::read_to_string(&path) {
                    if let Ok(mut agent) = serde_json::from_str::<serde_json::Value>(&content) {
                        // Normalize the agent definition
                        let identifier = agent
                            .get("identifier")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown")
                            .to_string();
                        let author = agent
                            .get("author")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown")
                            .to_string();
                        let homepage = agent
                            .get("homepage")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let locale = agent
                            .get("locale")
                            .and_then(|v| v.as_str())
                            .unwrap_or("en-US")
                            .to_string();

                        let meta = agent
                            .get("meta")
                            .map(|m| AgentMeta {
                                title: m
                                    .get("title")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or(&identifier)
                                    .to_string(),
                                description: m
                                    .get("description")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string(),
                                avatar: m
                                    .get("avatar")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("🤖")
                                    .to_string(),
                                tags: m
                                    .get("tags")
                                    .and_then(|v| v.as_array())
                                    .map(|arr| {
                                        arr.iter()
                                            .filter_map(|v| v.as_str().map(String::from))
                                            .collect()
                                    })
                                    .unwrap_or_default(),
                                category: m
                                    .get("category")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("defi")
                                    .to_string(),
                            })
                            .unwrap_or(AgentMeta {
                                title: identifier.clone(),
                                description: String::new(),
                                avatar: "🤖".to_string(),
                                tags: vec![],
                                category: "defi".to_string(),
                            });

                        let voice = agent.get("voice").cloned();

                        agents.push(AgentDefinition {
                            identifier,
                            author,
                            meta,
                            config: agent
                                .get("config")
                                .cloned()
                                .unwrap_or(json!({})),
                            homepage,
                            locale,
                            voice,
                        });
                    }
                }
            }
        }
    }

    agents
}

#[get("/api/agents/index.json")]
async fn get_agent_catalog() -> HttpResponse {
    let agents = load_all_agent_definitions();

    let entries: Vec<CatalogEntry> = agents
        .iter()
        .map(|a| CatalogEntry {
            identifier: a.identifier.clone(),
            title: a.meta.title.clone(),
            description: a.meta.description.clone(),
            avatar: a.meta.avatar.clone(),
            tags: a.meta.tags.clone(),
            category: a.meta.category.clone(),
            author: a.author.clone(),
            on_chain: false,
            mint_address: None,
        })
        .collect();

    let mut by_category = std::collections::HashMap::new();
    for entry in &entries {
        *by_category.entry(entry.category.clone()).or_insert(0) += 1;
    }

    let catalog = AgentCatalog {
        stats: CatalogStats {
            total: entries.len(),
            on_chain: 0,
            by_category,
        },
        categories: vec![
            CategoryDef { id: "voice-council".into(), label: "Lobster Council".into(), icon: "🦞".into() },
            CategoryDef { id: "defi".into(), label: "DeFi".into(), icon: "💰".into() },
            CategoryDef { id: "trading".into(), label: "Trading".into(), icon: "📈".into() },
            CategoryDef { id: "security".into(), label: "Security".into(), icon: "🛡️".into() },
            CategoryDef { id: "infrastructure".into(), label: "Infrastructure".into(), icon: "🏗️".into() },
        ],
        agents: entries,
    };

    HttpResponse::Ok().json(catalog)
}

#[get("/api/agents/{identifier}.json")]
async fn get_agent_by_id(path: web::Path<String>) -> HttpResponse {
    let identifier = path.into_inner();
    let agents = load_all_agent_definitions();

    if let Some(agent) = agents.into_iter().find(|a| a.identifier == identifier) {
        HttpResponse::Ok().json(agent)
    } else {
        HttpResponse::NotFound().json(json!({
            "error": format!("Agent '{}' not found", identifier)
        }))
    }
}

#[post("/api/agents/mint")]
async fn mint_agent(body: web::Json<MintAgentRequest>) -> HttpResponse {
    // TODO: Implement actual Metaplex MPL Core agent asset minting
    // For now, return a placeholder indicating the mint would happen
    let identifier = &body.identifier;
    let wallet = &body.wallet_address;

    HttpResponse::Ok().json(json!({
        "status": "simulated",
        "identifier": identifier,
        "wallet_address": wallet,
        "message": format!("Agent '{}' registered for on-chain mint. Wallet: {}", identifier, wallet),
        "mint_address": null,
        "tx_hash": null,
        "note": "On-chain minting requires Metaplex agent-registry SDK integration. This is a placeholder for the full implementation."
    }))
}

#[post("/api/agents/create")]
async fn create_agent(body: web::Json<serde_json::Value>) -> HttpResponse {
    let agents_dir = get_agents_dir();
    let identifier = body.get("identifier").and_then(|v| v.as_str()).unwrap_or("unknown");

    // Write the agent to the lobster-council directory
    if let Ok(content) = serde_json::to_string_pretty(&body.0) {
        let filepath = agents_dir.join(format!("{}.json", identifier));
        if let Err(e) = fs::write(&filepath, &content) {
            return HttpResponse::InternalServerError().json(json!({
                "error": format!("Failed to save agent: {}", e)
            }));
        }
        return HttpResponse::Ok().json(json!({
            "status": "created",
            "identifier": identifier,
            "path": filepath.to_string_lossy().to_string(),
        }));
    }

    HttpResponse::BadRequest().json(json!({ "error": "Invalid agent definition" }))
}

/// Health check
#[get("/api/agents/health")]
async fn agents_health() -> HttpResponse {
    let agents = load_all_agent_definitions();
    HttpResponse::Ok().json(json!({
        "status": "ok",
        "agent_count": agents.len(),
        "agents_dir": get_agents_dir().to_string_lossy().to_string(),
    }))
}