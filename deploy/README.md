# 部署服务器说明

Plotgram 使用双域名 + 双服务器部署：
- **plotgram.dev**（海外站）：承载 demo 页面，由 demo 站服务器提供
- **plotgram.cn**（国内主站，备案号 沪ICP备2026029910号-2）：镜像所有 demo 页面，由 shanxun 服务器提供
- CDN 与 Agent API 始终部署在 shanxun，同时通过 `assets.pg.agcli.cn` / `assets.plotgram.cn` 与 `api.pg.agcli.cn` / `api.plotgram.cn` 双域名暴露

## 总览

| 角色 | SSH 别名 | 域名 | 部署目录 |
|------|----------|------|----------|
| Demo 站（海外页面） | `plotgram.dev` | `demo.plotgram.dev` | `/var/www/plotgram` |
| 国内主站（页面镜像） | `shanxun` | `www.plotgram.cn` / `plotgram.cn` | `/var/www/plotgram.cn` |
| 资源 CDN | `shanxun` | `assets.pg.agcli.cn` / `assets.plotgram.cn` | `/var/www/assets.pg.agcli.cn` |
| Agent API | `shanxun` | `api.pg.agcli.cn` / `api.plotgram.cn` | `/opt/plotgram-agent-api` |
| 源码（编译用） | `shanxun` | — | `/opt/plotgram`（Rust 1.96 + rsproxy.cn） |

> **域名说明**：plotgram.cn 备案通过后，`api.plotgram.cn` 与 `assets.plotgram.cn` 作为 `api.pg.agcli.cn` / `assets.pg.agcli.cn` 的 server_name 别名指向同一份后端资源；`www.plotgram.cn` 与 `plotgram.cn` 是独立的国内主站，镜像 demo 站全部前端页面。

访问地址：

| 站点 | plotgram.dev | plotgram.cn |
|------|--------------|-------------|
| Website | https://demo.plotgram.dev/ | https://www.plotgram.cn/ |
| Playground | https://demo.plotgram.dev/playground/ | https://www.plotgram.cn/playground/ |
| Showcase | https://demo.plotgram.dev/showcase/ | https://www.plotgram.cn/showcase/ |
| Agent Demo | https://demo.plotgram.dev/agent/ | https://www.plotgram.cn/agent/ |
| Agent API | https://api.pg.agcli.cn/agent/chat | https://api.plotgram.cn/agent/chat |
| 静态资源 CDN | https://assets.pg.agcli.cn/ | https://assets.plotgram.cn/ |

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

## 2. 国内主站 — `shanxun`（plotgram.cn）

| 项目 | 值 |
|------|-----|
| SSH | `ssh shanxun` |
| 域名 | `www.plotgram.cn` / `plotgram.cn` |
| 部署路径 | `/var/www/plotgram.cn` |
| Web 服务 | nginx |
| HTTPS | Let's Encrypt（独立证书 `/etc/letsencrypt/live/plotgram.cn/`） |
| nginx 配置 | `/etc/nginx/conf.d/plotgram.cn.conf` |
| 配置模板 | [`deploy/nginx/plotgram.cn.conf`](nginx/plotgram.cn.conf) |

### 目录结构

镜像 demo 站（`/var/www/plotgram`）的全部前端页面：

```
/var/www/plotgram.cn/
├── playground/          # HTML、favicon、logo（不含 assets/、plotgram-wasm/）
├── agent/               # Agent Demo 页面（不含 assets/、plotgram-wasm/）
├── showcase/            # 页面、.pgm、manifest（不含 .svg）
└── assets/brand/        # 品牌 logo 等
```

### 路由

与 demo 站一致：`/` 302 跳转至 `/showcase/`，`/playground/`、`/agent/`、`/showcase/` 各自为 SPA。

### 与 demo 站的差异

- 静态资源（JS/CSS/WASM/SVG）仍走 `assets.pg.agcli.cn` CDN，无需重复部署
- Agent API 仍走 `api.pg.agcli.cn`（CORS 已加 plotgram.cn 白名单）
- 每次发布时由 `rsync_to_both` 同时同步到 demo 站与 plotgram.cn 镜像站

### 首次部署

plotgram.cn.conf 的 443 块引用 `/etc/letsencrypt/live/plotgram.cn/` 证书，证书签发前 nginx -t 会失败。因此首次部署需按以下顺序操作：

```bash
# 1. 在 shanxun 上创建目录
ssh shanxun 'mkdir -p /var/www/plotgram.cn /var/www/letsencrypt'

# 2. 用 standalone 模式签发 plotgram.cn 主站证书（需短暂停 nginx）
ssh shanxun 'systemctl stop nginx && \
    certbot certonly --standalone \
        -d plotgram.cn -d www.plotgram.cn \
        --email you@example.com --agree-tos -n && \
    systemctl start nginx'

# 3. 同步 nginx 配置（此时 plotgram.cn 证书已存在，nginx -t 可通过；
#    assets.pg.agcli.cn.conf 也已添加 assets.plotgram.cn server_name，
#    443 块暂时证书不匹配但 nginx -t 不校验 SAN 覆盖，能通过）
./deploy/deploy-website.sh --setup-nginx

# 4. 扩展 agcli.cn 证书以覆盖 plotgram.cn 的 api/assets 别名
#    （webroot 方式，port 80 块已在上一步同步并 reload，可服务 acme-challenge）
ssh shanxun 'certbot certonly --cert-name assets.pg.agcli.cn --expand \
    -d assets.pg.agcli.cn -d api.pg.agcli.cn \
    -d assets.plotgram.cn -d api.plotgram.cn \
    --webroot -w /var/www/letsencrypt -n'

# 5. 重载 nginx 使扩展后的证书生效
ssh shanxun 'nginx -t && systemctl reload nginx'

# 6. 更新服务器上 .env 的 CORS 白名单（加入 plotgram.cn origins）
ssh shanxun 'sed -i "s|^DEMO_ALLOWED_ORIGINS=.*|DEMO_ALLOWED_ORIGINS=https://demo.plotgram.dev,https://plotgram.cn,https://www.plotgram.cn,https://assets.pg.agcli.cn,https://api.pg.agcli.cn,https://assets.plotgram.cn,https://api.plotgram.cn|" /opt/plotgram-agent-api/.env'

# 7. 重启 Agent API 使新 CORS 生效
ssh shanxun 'cd /opt/plotgram-agent-api && ./stop.sh && ./start.sh'

# 8. 全量发布
./deploy/deploy-all.sh
```

---

## 3. 资源 CDN — `shanxun`

| 项目 | 值 |
|------|-----|
| SSH | `ssh shanxun` |
| 系统 | Alibaba Cloud Linux 3 |
| 公网 IP | `47.102.193.239` |
| 域名 | `assets.pg.agcli.cn` / `assets.plotgram.cn`（server_name 别名，同证书） |
| 部署路径 | `/var/www/assets.pg.agcli.cn` |
| Web 服务 | nginx |
| HTTPS | Let's Encrypt（certbot 自动续期，与 `api.pg.agcli.cn` / `api.plotgram.cn` 共用 SAN 证书） |
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
    └── **/*.svg        # 各类型示例 SVG
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

CDN 允许任意 Origin 跨域加载 WASM、JS、CSS、SVG：

```
Access-Control-Allow-Origin: *
```

已启用 gzip，含 `application/wasm` 类型。

---

## 4. Agent API — `shanxun`（api.pg.agcli.cn / api.plotgram.cn）

| 项目 | 值 |
|------|-----|
| SSH | `ssh shanxun` |
| 域名 | `api.pg.agcli.cn` / `api.plotgram.cn`（server_name 别名，同证书） |
| 部署路径 | `/opt/plotgram-agent-api` |
| 二进制 | `plotgram-server`（Linux x86_64, glibc 动态链接） |
| 编译位置 | `shanxun:/opt/plotgram`（Rust 1.96 + rsproxy.cn 镜像） |
| 监听 | `127.0.0.1:6080`（仅本地，nginx 反代） |
| nginx 配置 | `/etc/nginx/conf.d/api.pg.agcli.cn.conf` |
| 配置模板 | [`deploy/nginx/api.pg.agcli.cn.conf`](nginx/api.pg.agcli.cn.conf) |
| 环境变量 | `/opt/plotgram-agent-api/.env`（含 `DEEPSEEK_API_KEY`，权限 600，不进 git） |
| HTTPS 证书 | Let's Encrypt，与 `assets.pg.agcli.cn` / `assets.plotgram.cn` 共用（SAN 四域名） |

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

## 5. Agent Demo — `demo.plotgram.dev/agent/`（+ plotgram.cn 镜像）

Agent Demo（对话即画图）部署在 demo 站的 `/agent/` 子路径下，并同步镜像到 `www.plotgram.cn/agent/`。页面 HTML 在两台服务器，WASM 与打包 JS/CSS 走 CDN。Agent API 由第 4 节的 `api.pg.agcli.cn` / `api.plotgram.cn` 提供。

| 项目 | 值 |
|------|-----|
| 访问地址 | https://demo.plotgram.dev/agent/ 或 https://www.plotgram.cn/agent/ |
| 页面部署路径（主站） | `/var/www/plotgram/agent` |
| 页面部署路径（镜像） | `/var/www/plotgram.cn/agent` |
| CDN 路径 | `/var/www/assets.pg.agcli.cn/agent/` |
| nginx 配置 | `demo.plotgram.dev.conf` + `plotgram.cn.conf`（`/agent/` location）+ `assets.pg.agcli.cn.conf` |
| Agent API | https://api.pg.agcli.cn/agent/chat（CORS 允许 plotgram.cn / plotgram.dev） |
| 构建工具 | wasm-pack + vite build |
| WASM 来源 | `crates/plotgram-wasm`（与 playground 共用） |

### 构建参数

| 变量 | 值 | 说明 |
|------|-----|------|
| `VITE_BASE_PATH` | `/agent/` | vite base，页面与资源路径前缀 |
| `VITE_CDN_BASE` | `https://assets.pg.agcli.cn/agent/` | 打包 assets 与 wasm 的 CDN 根 |
| `VITE_AGENT_API` | `https://api.pg.agcli.cn/agent/chat` | Agent 中转 API（plotgram.cn 与 plotgram.dev 共用） |
| `VITE_WASM_BUILD_STAMP` | wasm md5 | WASM 缓存失效戳 |

### 首次部署

```bash
# 1. 确保 Agent API 已部署（见第 4 节）
./deploy/deploy-agent-api.sh

# 2. 同步 nginx 配置（添加 /agent/ location + CDN agent 路径 + plotgram.cn 镜像）
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

## 发布脚本

每个站点一个独立发布脚本，互不影响。公共逻辑（log/SSH 连接复用/rsync/nginx 同步）在 `lib/common.sh`。

### 脚本总览

| 脚本 | 站点 | 访问地址 |
|------|------|----------|
| `deploy-wasm.sh` | WASM（三端共用 CDN common） | `https://assets.pg.agcli.cn/plotgram-wasm/`（或 `https://assets.plotgram.cn/plotgram-wasm/`） |
| `deploy-website.sh` | Website（landing page） | `https://demo.plotgram.dev/` + `https://www.plotgram.cn/` |
| `deploy-playground.sh` | Playground | `https://demo.plotgram.dev/playground/` + `https://www.plotgram.cn/playground/` |
| `deploy-showcase.sh` | Showcase | `https://demo.plotgram.dev/showcase/` + `https://www.plotgram.cn/showcase/` |
| `deploy-agent-demo.sh` | Agent Demo | `https://demo.plotgram.dev/agent/` + `https://www.plotgram.cn/agent/` |
| `deploy-agent-api.sh` | Agent API（Rust 服务端） | `https://api.pg.agcli.cn/agent/chat`（或 `https://api.plotgram.cn/agent/chat`） |
| `deploy-all.sh` | 全量发布（按序调用上述脚本） | — |

### 全量发布

```bash
./deploy/deploy-all.sh                        # 全量发布（含 showcase SVG 渲染）
./deploy/deploy-all.sh --skip-render          # 跳过 showcase SVG 渲染
./deploy/deploy-all.sh --skip-api             # 跳过 agent-api（不编译 Rust 服务端）
./deploy/deploy-all.sh --only wasm,agent-demo # 只发布指定站点
```

发布顺序：wasm → website → playground → showcase → agent-demo → agent-api
（wasm 必须在 playground / agent-demo 之前，因为它们 build 时需要本地 wasm 副本）

### 单站点发布

```bash
# WASM（playground / agent-demo 三端共用，靠 ETag 控制缓存）
./deploy/deploy-wasm.sh
./deploy/deploy-wasm.sh --skip-build

# Website（landing page）
./deploy/deploy-website.sh
./deploy/deploy-website.sh --skip-build

# Playground
./deploy/deploy-playground.sh                 # 前置：需先 deploy-wasm.sh
./deploy/deploy-playground.sh --skip-build

# Showcase
./deploy/deploy-showcase.sh
./deploy/deploy-showcase.sh --skip-render     # 跳过 SVG 渲染

# Agent Demo
./deploy/deploy-agent-demo.sh                 # 前置：需先 deploy-wasm.sh
./deploy/deploy-agent-demo.sh --skip-build

# Agent API（shanxun 远程编译 + 重启）
./deploy/deploy-agent-api.sh
./deploy/deploy-agent-api.sh --skip-sync      # 跳过 rsync，用服务器已有代码编译
./deploy/deploy-agent-api.sh --skip-build     # 跳过编译，仅重启
```

所有前端站点脚本支持 `--setup-nginx` 同步 nginx 配置（首次部署或配置变更时使用）。

### 依赖关系

```
deploy-wasm.sh ──┬─→ deploy-playground.sh    （build 时需要 playground/plotgram-wasm/）
                  └─→ deploy-agent-demo.sh    （build 时需要 agent-demo/plotgram-wasm/）

deploy-website.sh    （独立）
deploy-showcase.sh   （独立，依赖 plotgram-cli）
deploy-agent-api.sh  （独立，远程编译 Rust 服务端）
```

### 环境变量

| 变量 | 默认值 | 说明 |
|------|--------|------|
| `DEPLOY_HOST` | `plotgram.dev`（agent-api 为 `shanxun`） | Demo 站 SSH 目标 |
| `ASSET_HOST` | `shanxun` | CDN SSH 目标 |
| `REMOTE_DIR` | `/var/www/plotgram` | Demo 站部署目录 |
| `ASSET_REMOTE_DIR` | `/var/www/assets.pg.agcli.cn` | CDN 部署目录 |
| `CDN_BASE` | `https://assets.pg.agcli.cn/` | 构建时注入的 CDN 根 URL |
| `SITE_MIRROR_HOST` | `shanxun` | plotgram.cn 镜像站 SSH 目标 |
| `SITE_MIRROR_DIR` | `/var/www/plotgram.cn` | plotgram.cn 镜像站部署目录 |
| `MIRROR_ENABLED` | `true` | 是否同步到 plotgram.cn 镜像站（设为 `false` 可临时禁用） |

### 同步策略

| 目标 | 同步内容 | 不同步 |
|------|----------|--------|
| Demo 站 `/`（plotgram.dev） | website 页面、品牌资源 | `assets/`（走 CDN）、其他站点子目录 |
| 镜像站 `/`（plotgram.cn） | 同上 | 同上 |
| Demo 站 `/playground/` | playground 页面 | `assets/`、wasm（走 CDN） |
| 镜像站 `/playground/` | 同上 | 同上 |
| Demo 站 `/showcase/` | showcase 页面、.pgm、manifest | `.svg`（走 CDN） |
| 镜像站 `/showcase/` | 同上 | 同上 |
| Demo 站 `/agent/` | agent 页面 | `assets/`、wasm（走 CDN） |
| 镜像站 `/agent/` | 同上 | 同上 |
| CDN `/plotgram-wasm/` | wasm 产物（三端共用） | — |
| CDN `/website/assets/` | website 打包 js/css | — |
| CDN `/playground/assets/` | playground 打包 js/css | — |
| CDN `/showcase/` | SVG 文件 | — |
| CDN `/agent/assets/` | agent 打包 js/css | — |

**关键**：website 脚本对两站根目录均使用 `--delete`，但已排除 `playground/` `showcase/` `agent/` 子目录，不会误删其他站点。镜像站通过 `rsync_to_both` 与主站保持同步。

WASM 缓存：CDN nginx 对 `/plotgram-wasm/` 使用 `no-cache` + ETag；前端以 `?v=<md5>` 加载 wasm。勿在 `public/plotgram-wasm/` 放置 wasm 副本。

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
