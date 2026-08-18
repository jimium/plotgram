//! Agent Demo — DeepSeek 中转 + 防滥用
//!
//! 提供 `POST /agent/chat`，作为 DeepSeek 的 SSE 流式中转。
//! 服务器持有 API Key，对浏览器不可见；并施加分层防滥用：
//!   L1 CORS + Origin/Referer
//!   L2 session_id 格式校验
//!   L3 IP 维度令牌桶 + session 维度请求次数
//!   L4 token 配额（单 session + 全局总额度池）
//!   L5 请求体约束（messages 字节数、max_tokens 上限、model 强制覆盖）
//!   L7 DEMO_ENABLED 一键开关
//!   L8 日志记录
//! L6 maxIterations 由前端 AgentLoop 硬编码（见 agent-demo）。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::{
    extract::{ConnectInfo, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
    Json,
};
use dashmap::DashMap;
use futures::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::net::SocketAddr;
use uuid::Uuid;

// ============ 错误响应 ============

#[derive(Debug, Serialize)]
pub struct ProxyError {
    pub error: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after: Option<u32>,
}

fn json_error(code: &'static str, message: impl Into<String>, status: StatusCode) -> Response {
    let body = ProxyError {
        error: code,
        message: message.into(),
        retry_after: None,
    };
    (status, Json(body)).into_response()
}

fn json_error_retry(
    code: &'static str,
    message: impl Into<String>,
    status: StatusCode,
    retry_after: u32,
) -> Response {
    let body = ProxyError {
        error: code,
        message: message.into(),
        retry_after: Some(retry_after),
    };
    (status, Json(body)).into_response()
}

// ============ 配置 ============

#[derive(Clone)]
pub struct AgentProxyConfig {
    pub enabled: bool,
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    pub total_token_budget: u64,
    pub per_session_token_budget: u64,
    pub per_session_request_limit: u32,
    pub max_tokens_per_request: u32,
    pub max_messages_bytes: usize,
    pub rate_limit_per_minute: u32,
    pub rate_limit_burst: u32,
    pub allowed_origins: Vec<String>,
    /// System prompt 指纹：messages[0] 必须以此前缀开头，防止 API 被挪作通用调用
    pub system_prompt_fingerprint: String,
}

impl AgentProxyConfig {
    pub fn from_env() -> Self {
        fn env_u64(key: &str, default: u64) -> u64 {
            std::env::var(key)
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(default)
        }
        fn env_u32(key: &str, default: u32) -> u32 {
            std::env::var(key)
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(default)
        }
        fn env_usize(key: &str, default: usize) -> usize {
            std::env::var(key)
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(default)
        }

        let allowed_origins = std::env::var("DEMO_ALLOWED_ORIGINS")
            .unwrap_or_default()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        Self {
            enabled: env_flag("DEMO_ENABLED", true),
            api_key: std::env::var("DEEPSEEK_API_KEY").unwrap_or_default(),
            base_url: std::env::var("DEEPSEEK_BASE_URL")
                .unwrap_or_else(|_| "https://api.deepseek.com".to_string()),
            model: std::env::var("DEEPSEEK_MODEL")
                .unwrap_or_else(|_| "deepseek-v4-flash".to_string()),
            total_token_budget: env_u64("DEMO_TOTAL_TOKEN_BUDGET", 2_000_000),
            per_session_token_budget: env_u64("DEMO_PER_SESSION_TOKEN_BUDGET", 30_000),
            per_session_request_limit: env_u32("DEMO_PER_SESSION_REQUEST_LIMIT", 30),
            max_tokens_per_request: env_u32("DEMO_MAX_TOKENS_PER_REQUEST", 4096),
            max_messages_bytes: env_usize("DEMO_MAX_MESSAGES_BYTES", 65_536),
            rate_limit_per_minute: env_u32("DEMO_RATE_LIMIT_PER_MINUTE", 10),
            rate_limit_burst: env_u32("DEMO_RATE_LIMIT_BURST", 3),
            allowed_origins,
            system_prompt_fingerprint: std::env::var("DEMO_SYSTEM_PROMPT_FINGERPRINT")
                .unwrap_or_else(|_| "你是 Tautcore Agent，一个\"对话即画图\"的 AI 助手".to_string()),
        }
    }
}

fn env_flag(key: &str, default: bool) -> bool {
    match std::env::var(key) {
        Ok(v) => matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"),
        Err(_) => default,
    }
}

// ============ 限流：IP 令牌桶 ============

struct TokenBucket {
    tokens: f64,
    last_refill: Instant,
}

impl TokenBucket {
    fn new(burst: u32) -> Self {
        Self {
            tokens: burst as f64,
            last_refill: Instant::now(),
        }
    }

    /// 尝试消费 1 个令牌，返回是否成功与距下次可用的秒数。
    fn try_consume(&mut self, rate_per_sec: f64, capacity: f64) -> (bool, u32) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.tokens = (self.tokens + elapsed * rate_per_sec).min(capacity);
        self.last_refill = now;

        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            (true, 0)
        } else {
            // 距下一个令牌的秒数
            let need = 1.0 - self.tokens;
            let secs = (need / rate_per_sec).ceil() as u32;
            (false, secs.max(1))
        }
    }
}

// ============ Session 状态 ============

struct SessionState {
    request_count: u32,
    total_tokens: u64,
}

// ============ 全局状态 ============

#[derive(Clone)]
pub struct AgentProxyState {
    inner: Arc<AgentProxyInner>,
}

struct AgentProxyInner {
    config: AgentProxyConfig,
    client: Client,
    ip_buckets: DashMap<String, TokenBucket>,
    sessions: DashMap<String, SessionState>,
    total_tokens_used: AtomicU64,
}

impl AgentProxyState {
    pub fn new(config: AgentProxyConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(300))
            .build()
            .expect("reqwest client");
        Self {
            inner: Arc::new(AgentProxyInner {
                config,
                client,
                ip_buckets: DashMap::new(),
                sessions: DashMap::new(),
                total_tokens_used: AtomicU64::new(0),
            }),
        }
    }
}

// ============ 请求体 ============

#[derive(Debug, Deserialize)]
pub struct AgentChatRequest {
    pub session_id: String,
    pub messages: Value,
    #[serde(default)]
    pub tools: Option<Value>,
    #[serde(default)]
    pub max_tokens: Option<u32>,
    #[serde(default)]
    pub temperature: Option<f32>,
    // model 由服务器强制覆盖（DEEPSEEK_MODEL），不读取前端传入值
}

// ============ Handler ============

pub async fn agent_chat_handler(
    State(state): State<AgentProxyState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<AgentChatRequest>,
) -> Response {
    let cfg = &state.inner.config;
    let started = Instant::now();

    // L7 — 一键开关
    if !cfg.enabled {
        return json_error("demo_disabled", "演示已关闭", StatusCode::SERVICE_UNAVAILABLE);
    }

    // 校验 API Key 是否配置
    if cfg.api_key.is_empty() {
        return json_error(
            "upstream_error",
            "服务器未配置 DEEPSEEK_API_KEY",
            StatusCode::INTERNAL_SERVER_ERROR,
        );
    }

    // L1 — Origin / Referer 校验
    if let Err(msg) = check_origin(&headers, &cfg.allowed_origins) {
        log_warn(&state, &headers, &body.session_id, "bad_origin", &msg);
        return json_error("bad_origin", msg, StatusCode::FORBIDDEN);
    }

    // L2 — session_id 格式校验（UUID v4）
    if Uuid::parse_str(&body.session_id).is_err() {
        return json_error(
            "session_invalid",
            "session_id 不是合法的 UUID",
            StatusCode::BAD_REQUEST,
        );
    }

    // L2b — System prompt 指纹校验
    // messages[0] 必须是 system 角色，且 content 以我们的指纹开头。
    // 防止 API 被挪作通用 LLM 调用；同时保证 system prompt 前缀一致，吃到 DeepSeek prefix cache。
    let fp_ok = body
        .messages
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|first| {
            let role = first.get("role").and_then(|v| v.as_str())?;
            let content = first.get("content").and_then(|v| v.as_str())?;
            Some(role == "system" && content.starts_with(&cfg.system_prompt_fingerprint))
        })
        .unwrap_or(false);
    if !fp_ok {
        log_warn(
            &state,
            &headers,
            &body.session_id,
            "bad_system_prompt",
            "messages[0] 不匹配预期的 system prompt 指纹",
        );
        return json_error(
            "bad_request",
            "请求格式不符合 Tautcore Agent 规范",
            StatusCode::BAD_REQUEST,
        );
    }

    // 提取客户端 IP（优先 X-Forwarded-For / X-Real-IP，适配 nginx 反代）
    let ip = extract_ip(&headers, addr).to_string();

    // L3a — IP 令牌桶
    let rate_per_sec = cfg.rate_limit_per_minute as f64 / 60.0;
    let capacity = cfg.rate_limit_burst.max(1) as f64;
    {
        let mut bucket = state
            .inner
            .ip_buckets
            .entry(ip.clone())
            .or_insert_with(|| TokenBucket::new(cfg.rate_limit_burst.max(1)));
        let (ok, retry_after) = bucket.try_consume(rate_per_sec, capacity);
        if !ok {
            log_warn(
                &state,
                &headers,
                &body.session_id,
                "rate_limited",
                &format!("ip={ip}"),
            );
            return json_error_retry(
                "rate_limited",
                "请求过于频繁，请稍后再试",
                StatusCode::TOO_MANY_REQUESTS,
                retry_after,
            );
        }
    }

    // L3b + L4 — session 维度请求次数 + token 配额预检
    {
        let session = state
            .inner
            .sessions
            .entry(body.session_id.clone())
            .or_insert_with(|| SessionState {
                request_count: 0,
                total_tokens: 0,
            });
        if session.request_count >= cfg.per_session_request_limit {
            return json_error(
                "quota_exceeded",
                format!(
                    "本会话请求次数已达上限 ({})",
                    cfg.per_session_request_limit
                ),
                StatusCode::TOO_MANY_REQUESTS,
            );
        }
        if session.total_tokens >= cfg.per_session_token_budget {
            return json_error(
                "quota_exceeded",
                "本会话 token 用量已达上限",
                StatusCode::TOO_MANY_REQUESTS,
            );
        }
        // 全局总额度池
        if state.inner.total_tokens_used.load(Ordering::Relaxed) >= cfg.total_token_budget {
            return json_error(
                "quota_exceeded",
                "演示总额度已用尽",
                StatusCode::TOO_MANY_REQUESTS,
            );
        }
    }

    // L5 — 请求体约束
    let messages_str = match serde_json::to_string(&body.messages) {
        Ok(s) => s,
        Err(_) => return json_error("upstream_error", "messages 序列化失败", StatusCode::BAD_REQUEST),
    };
    if messages_str.len() > cfg.max_messages_bytes {
        return json_error(
            "quota_exceeded",
            format!(
                "请求体过大 ({} > {})",
                messages_str.len(),
                cfg.max_messages_bytes
            ),
            StatusCode::BAD_REQUEST,
        );
    }

    // 强制 max_tokens 上限
    let max_tokens = body
        .max_tokens
        .unwrap_or(cfg.max_tokens_per_request)
        .min(cfg.max_tokens_per_request);

    let temperature = body.temperature.unwrap_or(0.7);

    // 构造上游请求体（强制 model，强制 stream + usage）
    // thinking: disabled — 演示场景关闭思考模式，更快更省（V4 默认开启思考）
    let mut upstream_body = serde_json::json!({
        "model": cfg.model,
        "messages": body.messages,
        "max_tokens": max_tokens,
        "temperature": temperature,
        "stream": true,
        "stream_options": { "include_usage": true },
        "thinking": { "type": "disabled" },
    });
    if let Some(tools) = body.tools {
        if !tools_is_empty(&tools) {
            upstream_body["tools"] = tools;
            upstream_body["tool_choice"] = serde_json::json!("auto");
        }
    }

    // 向 DeepSeek 发起流式请求
    let endpoint = format!(
        "{}/chat/completions",
        cfg.base_url.trim_end_matches('/')
    );
    let upstream_res = match state
        .inner
        .client
        .post(&endpoint)
        .header("Authorization", format!("Bearer {}", cfg.api_key))
        .header("Content-Type", "application/json")
        .header("Accept", "text/event-stream")
        .json(&upstream_body)
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            log_warn(
                &state,
                &headers,
                &body.session_id,
                "upstream_error",
                &e.to_string(),
            );
            return json_error(
                "upstream_error",
                format!("DeepSeek 请求失败: {e}"),
                StatusCode::BAD_GATEWAY,
            );
        }
    };

    if !upstream_res.status().is_success() {
        let status = upstream_res.status();
        let text = upstream_res.text().await.unwrap_or_default();
        log_warn(
            &state,
            &headers,
            &body.session_id,
            "upstream_error",
            &format!("upstream status={status} body={text}"),
        );
        return json_error(
            "upstream_error",
            format!("DeepSeek 返回 {status}"),
            StatusCode::BAD_GATEWAY,
        );
    }

    // session 请求计数 +1
    if let Some(mut session) = state.inner.sessions.get_mut(&body.session_id) {
        session.request_count += 1;
    }

    // 透传 SSE 流，并解析 usage 累加配额
    let session_id = body.session_id.clone();
    let state_clone = state.clone();
    let upstream_stream = upstream_res.bytes_stream();

    let event_stream = async_stream::stream! {
        let mut byte_buffer: Vec<u8> = Vec::with_capacity(8192);
        let mut text_buffer = String::new();
        let mut acc_tokens: u64 = 0;
        let mut reader = upstream_stream;

        while let Some(chunk_res) = reader.next().await {
            match chunk_res {
                Ok(bytes) => {
                    byte_buffer.extend_from_slice(&bytes);
                    // 按 SSE 事件边界 (\n\n) 切分
                    loop {
                        let Some(idx) = find_double_newline(&byte_buffer) else { break; };
                        let event_bytes: Vec<u8> = byte_buffer.drain(..idx + 2).collect();
                        text_buffer.push_str(&String::from_utf8_lossy(&event_bytes));

                        // 从事件块中提取 data: 行
                        if let Some(data_payload) = extract_data_line(&text_buffer) {
                            // 解析 usage 用于配额计量
                            if let Some(usage) = parse_usage(&data_payload) {
                                acc_tokens +=
                                    (usage.prompt_tokens as u64) + (usage.completion_tokens as u64);
                            }
                            // 透传给客户端
                            yield Ok::<_, std::convert::Infallible>(
                                Event::default().data(&data_payload),
                            );
                        }
                        text_buffer.clear();
                    }
                }
                Err(e) => {
                    eprintln!("[agent_proxy] upstream stream error: {e}");
                    break;
                }
            }
        }
        // 刷新残余 buffer
        if !byte_buffer.is_empty() {
            text_buffer.push_str(&String::from_utf8_lossy(&byte_buffer));
            if let Some(data_payload) = extract_data_line(&text_buffer) {
                if let Some(usage) = parse_usage(&data_payload) {
                    acc_tokens +=
                        (usage.prompt_tokens as u64) + (usage.completion_tokens as u64);
                }
                yield Ok::<_, std::convert::Infallible>(
                    Event::default().data(&data_payload),
                );
            }
        }

        // 记账：session + 全局
        if acc_tokens > 0 {
            if let Some(mut session) = state_clone.inner.sessions.get_mut(&session_id) {
                session.total_tokens = session.total_tokens.saturating_add(acc_tokens);
            }
            state_clone
                .inner
                .total_tokens_used
                .fetch_add(acc_tokens, Ordering::Relaxed);
            let total = state_clone.inner.total_tokens_used.load(Ordering::Relaxed);
            eprintln!(
                "[agent_proxy] session={session_id} tokens=+{acc_tokens} (session total may exceed; global used={total})"
            );
        }

        let elapsed_ms = started.elapsed().as_millis();
        eprintln!(
            "[agent_proxy] done session={session_id} elapsed={elapsed_ms}ms tokens={acc_tokens}"
        );
    };

    Sse::new(event_stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

// ============ 辅助函数 ============

fn check_origin(headers: &HeaderMap, allowed: &[String]) -> Result<(), String> {
    if allowed.is_empty() {
        // 未配置白名单时放行（开发场景），但仍记录
        return Ok(());
    }
    let origin = headers
        .get("origin")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let referer = headers
        .get("referer")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    // 两者皆空 → 拒绝（挡 curl 等非浏览器请求）
    if origin.is_empty() && referer.is_empty() {
        return Err("缺少 Origin/Referer 头".to_string());
    }

    let origin_ok = !origin.is_empty() && allowed.iter().any(|a| a == origin);
    let referer_ok = !referer.is_empty()
        && allowed
            .iter()
            .any(|a| referer.starts_with(a.as_str()));
    if origin_ok || referer_ok {
        Ok(())
    } else {
        Err(format!("来源不在白名单 (origin={origin}, referer={referer})"))
    }
}

fn extract_ip(headers: &HeaderMap, addr: SocketAddr) -> String {
    if let Some(v) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
        // 取第一个 IP
        return v.split(',').next().unwrap_or("").trim().to_string();
    }
    if let Some(v) = headers.get("x-real-ip").and_then(|v| v.to_str().ok()) {
        return v.trim().to_string();
    }
    addr.ip().to_string()
}

fn find_double_newline(buf: &[u8]) -> Option<usize> {
    // 查找 \n\n
    buf.windows(2).position(|w| w == b"\n\n")
}

/// 从一个 SSE 事件块（可能含多行）中提取 `data: ` 后的载荷。
fn extract_data_line(block: &str) -> Option<String> {
    for line in block.lines() {
        if let Some(rest) = line.strip_prefix("data:") {
            return Some(rest.trim_start_matches(' ').to_string());
        }
    }
    None
}

#[derive(Deserialize)]
struct UsagePayload {
    #[serde(default)]
    usage: Option<UsageCounts>,
}

#[derive(Deserialize)]
struct UsageCounts {
    #[serde(default)]
    prompt_tokens: u32,
    #[serde(default)]
    completion_tokens: u32,
}

fn parse_usage(data: &str) -> Option<UsageCounts> {
    if data == "[DONE]" {
        return None;
    }
    let parsed: UsagePayload = serde_json::from_str(data).ok()?;
    parsed.usage
}

fn tools_is_empty(tools: &Value) -> bool {
    match tools {
        Value::Array(a) => a.is_empty(),
        Value::Null => true,
        _ => false,
    }
}

fn log_warn(
    state: &AgentProxyState,
    _headers: &HeaderMap,
    session_id: &str,
    code: &str,
    detail: &str,
) {
    let total = state.inner.total_tokens_used.load(Ordering::Relaxed);
    eprintln!(
        "[agent_proxy] reject code={code} session={session_id} global_tokens={total} detail={detail}"
    );
}

// ============ CORS（仅 /agent/* 生效）============

/// 构造 /agent/* 的 CORS 层。
/// 若 DEMO_ALLOWED_ORIGINS 配置，则仅允许这些来源；否则允许任意（开发场景）。
pub fn agent_cors_layer(config: &AgentProxyConfig) -> tower_http::cors::CorsLayer {
    use tower_http::cors::{Any, CorsLayer};
    let mut cors = CorsLayer::new()
        .allow_methods([
            axum::http::Method::GET,
            axum::http::Method::POST,
            axum::http::Method::OPTIONS,
        ])
        .allow_headers([
            axum::http::header::CONTENT_TYPE,
            axum::http::header::ACCEPT,
            axum::http::HeaderName::from_static("x-demo-session"),
        ]);
    if config.allowed_origins.is_empty() {
        cors = cors.allow_origin(Any);
    } else {
        let origins: Vec<HeaderValue> = config
            .allowed_origins
            .iter()
            .filter_map(|o| HeaderValue::from_str(o).ok())
            .collect();
        cors = cors.allow_origin(origins);
    }
    cors
}
