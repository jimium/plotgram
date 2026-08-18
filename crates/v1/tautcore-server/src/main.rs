//! Tautcore Server
//!
//! Web API 服务，提供 Tautcore 校验与渲染接口，以及 Agent Demo 的 DeepSeek 中转。

mod agent_proxy;
mod api;

use axum::{routing::get, routing::post, Router};
use std::env;

async fn health() -> &'static str {
    "ok"
}

#[tokio::main]
async fn main() {
    // Agent Proxy 配置（即使未启用也构造，便于路由注册）
    let proxy_config = agent_proxy::AgentProxyConfig::from_env();
    let proxy_state = agent_proxy::AgentProxyState::new(proxy_config.clone());
    let cors = agent_proxy::agent_cors_layer(&proxy_config);

    let app = Router::new()
        .route("/health", get(health))
        .route("/validate", post(api::validate_handler))
        .route("/render", post(api::render_handler))
        .route("/agent/chat", post(agent_proxy::agent_chat_handler))
        .layer(cors)
        .with_state(proxy_state)
        .into_make_service_with_connect_info::<std::net::SocketAddr>();

    let addr = env::var("TAUTCORE_SERVER_ADDR").unwrap_or_else(|_| "0.0.0.0:6080".to_string());
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .unwrap_or_else(|err| {
            eprintln!("错误: 无法绑定地址 '{addr}': {err}");
            std::process::exit(1);
        });

    println!("Tautcore Server listening on {addr}");
    println!("  POST /validate    — 语法与语义校验");
    println!("  POST /render      — 渲染 (svg/ascii/json)");
    println!("  POST /agent/chat  — DeepSeek 中转 (SSE) [enabled={}]", proxy_config.enabled);
    println!("  GET  /health      — 健康检查");

    axum::serve(listener, app).await.unwrap();
}
