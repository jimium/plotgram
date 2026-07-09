# 部署服务器说明

Plotgram Demo 使用两台服务器：海外站承载页面，国内 CDN 承载大体积静态资源。

## 总览

| 角色 | SSH 别名 | 域名 | 部署目录 |
|------|----------|------|----------|
| Demo 站（页面） | `plotgram.dev` | `demo.plotgram.dev` | `/var/www/plotgram` |
| 资源 CDN | `shanxun` | `assets.pg.agcli.cn` | `/var/www/assets.pg.agcli.cn` |
| Agent API | `shanxun` | `api.pg.agcli.cn` | `/opt/plotgram-agent-api` |
| 源码（编译用） | `shanxun` | — | `/opt/plotgram`（Rust 1.96 + rsproxy.cn） |

> **域名说明**：原计划用 `assets.plotgram.cn` / `demo-api.plotgram.cn`，但 plotgram.cn 未备案被阿里云 WAF 拦截。改用已备案的 `agcli.cn` 子域名（`assets.pg.agcli.cn` + `api.pg.agcli.cn`）。

访问地址：

- Playground: https://demo.plotgram.dev/playground/
- Showcase: https://demo.plotgram.dev/showcase/
- 静态资源 CDN: https://assets.pg.agcli.cn/
- Agent API: https://api.pg.agcli.cn/agent/chat

---

## 1. Demo 站 — `plotgram.dev`

| 项目 | 值 |
|------|-----|
| SSH | `ssh plotgram.dev` |
| 系统 | Ubuntu 26.04 LTS |
| 公网 IP | `45.77.27.170` |
| 域名 | `demo.plotgram.dev` |
| 部署路径 | `/var/www/plotgram` |
| Web 服务 | nginx |
| HTTPS | Let's Encrypt（certbot 自动续期） |
| nginx 配置 | `/etc/nginx/conf.d/demo.plotgram.dev.conf` |
| 配置模板 | [`deploy/nginx/demo.plotgram.dev.conf`](nginx/demo.plotgram.dev.conf) |

### 目录结构

```
/var/www/plotgram/
├── playground/          # HTML、favicon、logo（不含 assets/、plotgram-wasm/）
├── agent/               # Agent Demo 页面（不含 assets/、plotgram-wasm/）
├── showcase/            # 页面、.pgm、manifest（不含 .svg）
└── assets/brand/        # 品牌 logo 等
```

### 路由

| 路径 | 说明 |
|------|------|
| `/` | 302 跳转到 `/showcase/` |
| `/playground/` | Playground SPA |
| `/agent/` | Agent Demo（对话即画图） |
| `/showcase/` | Showcase 画廊 |
| `/assets/` | 品牌静态资源 |

---

## 2. 资源 CDN — `shanxun`

| 项目 | 值 |
|------|-----|
| SSH | `ssh shanxun` |
| 系统 | Alibaba Cloud Linux 3 |
| 公网 IP | `47.102.193.239` |
| 域名 | `assets.pg.agcli.cn` |
| 部署路径 | `/var/www/assets.pg.agcli.cn` |
| Web 服务 | nginx |
| HTTPS | Let's Encrypt（certbot 自动续期，与 `api.pg.agcli.cn` 共用 SAN 证书） |
| nginx 配置 | `/etc/nginx/conf.d/assets.pg.agcli.cn.conf` |
| 配置模板 | [`deploy/nginx/assets.pg.agcli.cn.conf`](nginx/assets.pg.agcli.cn.conf) |

### 目录结构

```
/var/www/assets.pg.agcli.cn/
├── playground/
│   ├── assets/         # 打包 js / css（main-*.js、wasm-*.js 等）
│   └── plotgram-wasm/   # plotgram_wasm.js、plotgram_wasm_bg.wasm
├── agent/
│   ├── assets/         # Agent Demo 打包 js / css
│   └── plotgram-wasm/   # 与 playground 共用同一份 wasm 产物
└── showcase/
    ├── **/*.svg        # 各类型示例 SVG
    └── .history/       # SVG 历史快照
```

### CDN URL 映射

| 资源 | URL 示例 |
|------|----------|
| Playground JS/CSS | `https://assets.pg.agcli.cn/playground/assets/main-*.js` |
| WASM JS | `https://assets.pg.agcli.cn/playground/plotgram-wasm/plotgram_wasm.js` |
| WASM 二进制 | `https://assets.pg.agcli.cn/playground/plotgram-wasm/plotgram_wasm_bg.wasm` |
| Agent Demo JS/CSS | `https://assets.pg.agcli.cn/agent/assets/main-*.js` |
| Agent Demo WASM | `https://assets.pg.agcli.cn/agent/plotgram-wasm/plotgram_wasm_bg.wasm` |
| Showcase SVG | `https://assets.pg.agcli.cn/showcase/flowchart/s.linear-chain.svg` |

### 跨域（CORS）

CDN 允许 `https://demo.plotgram.dev` 跨域加载 WASM、JS、CSS、SVG：

```
Access-Control-Allow-Origin: https://demo.plotgram.dev
```

已启用 gzip，含 `application/wasm` 类型。

---

## 3. Agent API — `shanxun`（api.pg.agcli.cn）

| 项目 | 值 |
|------|-----|
| SSH | `ssh shanxun` |
| 域名 | `api.pg.agcli.cn`（已备案，独立域名） |
| 部署路径 | `/opt/plotgram-agent-api` |
| 二进制 | `plotgram-server`（Linux x86_64, glibc 动态链接） |
| 编译位置 | `shanxun:/opt/plotgram`（Rust 1.96 + rsproxy.cn 镜像） |
| 监听 | `127.0.0.1:6080`（仅本地，nginx 反代） |
| nginx 配置 | `/etc/nginx/conf.d/api.pg.agcli.cn.conf` |
| 配置模板 | [`deploy/nginx/api.pg.agcli.cn.conf`](nginx/api.pg.agcli.cn.conf) |
| 环境变量 | `/opt/plotgram-agent-api/.env`（含 `DEEPSEEK_API_KEY`，权限 600，不进 git） |
| HTTPS 证书 | Let's Encrypt，与 `assets.pg.agcli.cn` 共用（SAN 双域名） |

### 架构

```
浏览器 → HTTPS → api.pg.agcli.cn(443)
                    │
                    ├─ /agent/chat → 127.0.0.1:6080(plotgram-server) → DeepSeek API
                    │                    ↑                              ↑
                    │               SSE 透传                    持有 API Key + 8 层防滥用
                    │
                    └─ /health → 127.0.0.1:6080/health
```

API Key (`DEEPSEEK_API_KEY`) 仅存在于服务器的 `.env` 文件中，永不下发到浏览器。

### 目录结构

```
/opt/plotgram-agent-api/     # 部署目录
├── plotgram-server       # Linux 二进制（shanxun 本地编译）
├── .env                  # 环境变量（含 API Key，权限 600）
├── .env.example          # 配置模板
├── start.sh              # 启动脚本（nohup 后台 + PID 文件）
├── stop.sh               # 停止脚本（SIGTERM → SIGKILL）
├── plotgram-server.pid   # PID 文件（运行时生成）
└── server.log            # 日志（运行时生成）

/opt/plotgram/               # 源码目录（常驻，rsync 增量同步）
├── Cargo.toml
├── crates/...
└── target/release/plotgram-server  # 编译产物
```

### 路由

| 路径 | 说明 |
|------|------|
| `GET /health` | 健康检查 |
| `POST /agent/chat` | Agent 中转 API（SSE 流式） |

### 服务管理

```bash
# 启动
ssh shanxun 'cd /opt/plotgram-agent-api && ./start.sh'

# 停止
ssh shanxun 'cd /opt/plotgram-agent-api && ./stop.sh'

# 重启
ssh shanxun 'cd /opt/plotgram-agent-api && ./stop.sh && ./start.sh'

# 查看日志
ssh shanxun 'tail -f /opt/plotgram-agent-api/server.log'
```

### 首次部署

```bash
# 1. 同步 nginx 配置 + 申请 certbot 证书
./deploy/deploy-agent-api.sh --setup-nginx

# 2. 同步代码 + 编译 + 部署 + 重启
./deploy/deploy-agent-api.sh

# 3. 登录服务器填入真实 API Key（如未在 .env 中）
ssh shanxun 'vim /opt/plotgram-agent-api/.env'
# 修改 DEEPSEEK_API_KEY=sk-xxx

# 4. 重启服务使 .env 生效
ssh shanxun 'cd /opt/plotgram-agent-api && ./stop.sh && ./start.sh'
```

### 后续发布

```bash
# 同步代码 + 编译 + 部署 + 重启（每次改代码后执行）
./deploy/deploy-agent-api.sh

# 跳过 rsync，用服务器上已有代码编译 + 部署
./deploy/deploy-agent-api.sh --skip-sync

# 跳过编译，仅重启（用已有二进制）
./deploy/deploy-agent-api.sh --skip-build

# 只同步代码不编译/不重启
./deploy/deploy-agent-api.sh --dry-run
```

### 编译流程（shanxun 本地编译）

shanxun 已安装 Rust 1.96 + rsproxy.cn 镜像，编译在服务器本地进行，无需交叉编译：

```
本机 rsync 源码 → shanxun:/opt/plotgram/（增量同步，~5s）
shanxun cd /opt/plotgram && cargo build --release -p plotgram-server  # rsproxy.cn 镜像，~4-5 分钟
shanxun cp target/release/plotgram-server → /opt/plotgram-agent-api/plotgram-server.new
shanxun /opt/plotgram-agent-api/stop.sh && mv *.new → plotgram-server && start.sh
```

shanxun 的 `~/.cargo/config.toml` 已配置 rsproxy.cn 镜像，编译速度远快于直连 crates.io。

---

## 4. Agent Demo — `demo.plotgram.dev/agent/`

Agent Demo（对话即画图）部署在 demo 站的 `/agent/` 子路径下，页面 HTML 在 demo 站，WASM 与打包 JS/CSS 走 CDN。Agent API 由第 3 节的 `api.pg.agcli.cn` 提供。

| 项目 | 值 |
|------|-----|
| 访问地址 | https://demo.plotgram.dev/agent/ |
| 页面部署路径 | `/var/www/plotgram/agent` |
| CDN 路径 | `/var/www/assets.pg.agcli.cn/agent/` |
| nginx 配置 | `demo.plotgram.dev.conf`（`/agent/` location）+ `assets.pg.agcli.cn.conf` |
| Agent API | https://api.pg.agcli.cn/agent/chat |
| 构建工具 | wasm-pack + vite build |
| WASM 来源 | `crates/plotgram-wasm`（与 playground 共用） |

### 构建参数

| 变量 | 值 | 说明 |
|------|-----|------|
| `VITE_BASE_PATH` | `/agent/` | vite base，页面与资源路径前缀 |
| `VITE_CDN_BASE` | `https://assets.pg.agcli.cn/agent/` | 打包 assets 与 wasm 的 CDN 根 |
| `VITE_AGENT_API` | `https://api.pg.agcli.cn/agent/chat` | Agent 中转 API |
| `VITE_WASM_BUILD_STAMP` | wasm md5 | WASM 缓存失效戳 |

### 首次部署

```bash
# 1. 确保 Agent API 已部署（见第 3 节）
./deploy/deploy-agent-api.sh

# 2. 同步 nginx 配置（添加 /agent/ location + CDN agent 路径）
./deploy/deploy-agent-demo.sh --setup-nginx

# 3. 构建 + 部署
./deploy/deploy-agent-demo.sh
```

### 后续发布

```bash
./deploy/deploy-agent-demo.sh                # 构建 + 同步
./deploy/deploy-agent-demo.sh --skip-build   # 跳过构建，用已有 dist 同步
./deploy/deploy-agent-demo.sh --setup-nginx  # 同步 nginx 配置（配置变更时）
```

### WASM 加载策略

与 playground 一致：构建时把 `plotgram_wasm_bg.wasm` 的 md5 注入 `VITE_WASM_BUILD_STAMP`，前端以 `?v=<md5>` 显式加载 wasm JS 与二进制；CDN nginx 对 `/agent/plotgram-wasm/` 使用 `no-cache`。

---

## 一键发布

### Demo 站 + CDN

```bash
./deploy/deploy-demo.sh
./deploy/deploy-demo.sh --skip-showcase-render   # 跳过 showcase SVG 重新渲染
```

脚本流程：本地构建 → 同步 demo 站 → 同步 CDN。

### Agent API

```bash
./deploy/deploy-agent-api.sh              # shanxun 本地编译 + 部署 + 重启
./deploy/deploy-agent-api.sh --setup-nginx # 同步 nginx 配置（首次或配置变更时）
./deploy/deploy-agent-api.sh --skip-sync  # 跳过 rsync，用服务器已有代码编译
./deploy/deploy-agent-api.sh --skip-build # 跳过编译，仅重启
```

脚本流程：rsync 源码到 shanxun:/opt/plotgram → cargo build（rsproxy.cn 镜像）→ 复制二进制 → stop/start 重启 → 健康检查。

### Agent Demo

```bash
./deploy/deploy-agent-demo.sh                # wasm-pack + vite build + 同步
./deploy/deploy-agent-demo.sh --skip-build   # 跳过构建，用已有 dist 同步
./deploy/deploy-agent-demo.sh --setup-nginx  # 同步 nginx 配置（首次或配置变更时）
```

脚本流程：wasm-pack 构建 WASM → vite build（注入 CDN base 与 wasm stamp）→ 同步页面到 demo 站 `/agent/` → 同步 wasm/assets 到 CDN `/agent/` → 验证。

### 环境变量

| 变量 | 默认值 | 说明 |
|------|--------|------|
| `DEPLOY_HOST` | `plotgram.dev` | Demo 站 SSH 目标 |
| `ASSET_HOST` | `shanxun` | CDN SSH 目标 |
| `REMOTE_DIR` | `/var/www/plotgram` | Demo 站部署目录 |
| `ASSET_REMOTE_DIR` | `/var/www/assets.pg.agcli.cn` | CDN 部署目录 |
| `CDN_BASE` | `https://assets.pg.agcli.cn/` | 构建时注入的 CDN 根 URL |

### 同步策略

| 目标 | 同步内容 | 不同步 |
|------|----------|--------|
| Demo 站 | playground 页面、showcase 页面与 .pgm、品牌资源 | `assets/`、wasm、showcase SVG |
| CDN | wasm、playground 打包 assets、showcase SVG 及历史快照 | 页面与 .pgm |

WASM 缓存：构建时把 `plotgram_wasm_bg.wasm` 的 md5 注入 `VITE_WASM_BUILD_STAMP`，前端以 `?v=<md5>` 加载 wasm；CDN nginx 对 `/playground/plotgram-wasm/` 使用 `no-cache`。勿在 `public/plotgram-wasm/` 放置 wasm 副本。

---

## 运维备忘

### 重载 nginx

```bash
ssh plotgram.dev  'nginx -t && systemctl reload nginx'
ssh shanxun       'nginx -t && systemctl reload nginx'
```

### 证书续期

两台服务器均已配置 certbot timer，一般无需手动操作。如需手动续期：

```bash
ssh plotgram.dev  'certbot renew'
ssh shanxun       'certbot renew'
```

### 防火墙

- **plotgram.dev**：UFW 已开放 22、80、443
- **shanxun**：按需确认安全组 / 防火墙放行 80、443
