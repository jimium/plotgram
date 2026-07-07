# 部署服务器说明

Plotgram Demo 使用两台服务器：海外站承载页面，国内 CDN 承载大体积静态资源。

## 总览

| 角色 | SSH 别名 | 域名 | 部署目录 |
|------|----------|------|----------|
| Demo 站（页面） | `plotgram.dev` | `demo.plotgram.dev` | `/var/www/plotgram` |
| 资源 CDN | `shanxun` | `assets.plotgram.cn` | `/var/www/assets.plotgram.cn` |

访问地址：

- Playground: https://demo.plotgram.dev/playground/
- Showcase: https://demo.plotgram.dev/showcase/
- 静态资源 CDN: https://assets.plotgram.cn/

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
├── showcase/            # 页面、.pgm、manifest（不含 .svg）
└── assets/brand/        # 品牌 logo 等
```

### 路由

| 路径 | 说明 |
|------|------|
| `/` | 302 跳转到 `/showcase/` |
| `/playground/` | Playground SPA |
| `/showcase/` | Showcase 画廊 |
| `/assets/` | 品牌静态资源 |

---

## 2. 资源 CDN — `shanxun`

| 项目 | 值 |
|------|-----|
| SSH | `ssh shanxun` |
| 系统 | Alibaba Cloud Linux 3 |
| 公网 IP | `47.102.193.239` |
| 域名 | `assets.plotgram.cn` |
| 部署路径 | `/var/www/assets.plotgram.cn` |
| Web 服务 | nginx |
| HTTPS | Let's Encrypt（certbot 自动续期） |
| nginx 配置 | `/etc/nginx/conf.d/assets.plotgram.cn.conf` |
| 配置模板 | [`deploy/nginx/assets.plotgram.cn.conf`](nginx/assets.plotgram.cn.conf) |

### 目录结构

```
/var/www/assets.plotgram.cn/
├── playground/
│   ├── assets/         # 打包 js / css（main-*.js、wasm-*.js 等）
│   └── plotgram-wasm/   # plotgram_wasm.js、plotgram_wasm_bg.wasm
└── showcase/
    ├── **/*.svg        # 各类型示例 SVG
    └── .history/       # SVG 历史快照
```

### CDN URL 映射

| 资源 | URL 示例 |
|------|----------|
| Playground JS/CSS | `https://assets.plotgram.cn/playground/assets/main-*.js` |
| WASM JS | `https://assets.plotgram.cn/playground/plotgram-wasm/plotgram_wasm.js` |
| WASM 二进制 | `https://assets.plotgram.cn/playground/plotgram-wasm/plotgram_wasm_bg.wasm` |
| Showcase SVG | `https://assets.plotgram.cn/showcase/flowchart/s.linear-chain.svg` |

### 跨域（CORS）

CDN 允许 `https://demo.plotgram.dev` 跨域加载 WASM、JS、CSS、SVG：

```
Access-Control-Allow-Origin: https://demo.plotgram.dev
```

已启用 gzip，含 `application/wasm` 类型。

---

## 一键发布

```bash
./deploy/deploy-demo.sh
./deploy/deploy-demo.sh --skip-showcase-render   # 跳过 showcase SVG 重新渲染
```

脚本流程：本地构建 → 同步 demo 站 → 同步 CDN。

### 环境变量

| 变量 | 默认值 | 说明 |
|------|--------|------|
| `DEPLOY_HOST` | `plotgram.dev` | Demo 站 SSH 目标 |
| `ASSET_HOST` | `shanxun` | CDN SSH 目标 |
| `REMOTE_DIR` | `/var/www/plotgram` | Demo 站部署目录 |
| `ASSET_REMOTE_DIR` | `/var/www/assets.plotgram.cn` | CDN 部署目录 |
| `CDN_BASE` | `https://assets.plotgram.cn/` | 构建时注入的 CDN 根 URL |

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
