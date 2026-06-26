//! Drawify Server
//!
//! Web API 服务，提供 Drawify 校验与渲染接口。

mod api;

use axum::{routing::get, routing::post, Router};
use drawify_core::render::encode::{fonts_dir, set_fonts_dir};
use std::env;
use std::path::PathBuf;

async fn health() -> &'static str {
    "ok"
}

fn configure_fonts_dir() {
    if let Ok(dir) = env::var("DRAWIFY_FONTS_DIR") {
        let dir = dir.trim();
        if !dir.is_empty() {
            set_fonts_dir(PathBuf::from(dir));
        }
    }

    let dir = fonts_dir();
    if !dir.is_dir() {
        eprintln!(
            "警告: 字体目录不存在 '{}'，PNG/WebP 渲染中的中文可能显示异常",
            dir.display()
        );
    }
}

#[tokio::main]
async fn main() {
    configure_fonts_dir();

    let app = Router::new()
        .route("/health", get(health))
        .route("/validate", post(api::validate_handler))
        .route("/render", post(api::render_handler));

    let addr = env::var("DRAWIFY_SERVER_ADDR").unwrap_or_else(|_| "0.0.0.0:6080".to_string());
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .unwrap_or_else(|err| {
            eprintln!("错误: 无法绑定地址 '{addr}': {err}");
            std::process::exit(1);
        });

    println!("Drawify Server listening on {addr}");
    println!("  POST /validate  — 语法与语义校验");
    println!("  POST /render    — 渲染 (svg/ascii/png/webp/json)");
    println!("  GET  /health    — 健康检查");

    axum::serve(listener, app).await.unwrap();
}
