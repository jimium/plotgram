# 部署流程

## 1. 构建产物

Drawify Studio 是纯前端应用,构建产物为静态文件,可部署到任意静态文件服务器。

### 1.1 前置条件

- Node.js 18+
- 已构建 drawify-wasm 产物(见下文)

### 1.2 构建 WASM 产物

Studio 依赖 drawify-wasm,需先在仓库根目录构建:

```bash
cd /path/to/flowml/crates/drawify-wasm
wasm-pack build --target web --release --out-dir ../../studio/drawify-wasm
```

构建后 `studio/drawify-wasm/` 目录包含:
- `drawify_wasm.js`(JS 胶水代码)
- `drawify_wasm_bg.wasm`(WASM 二进制)
- `drawify_wasm.d.ts`(类型声明)

> 注意:此目录已在 `.gitignore` 中,不入版本控制。

### 1.3 构建前端

```bash
cd studio
npm install
npm run build
```

构建产物在 `studio/dist/`,包含:
- `index.html`
- `assets/`(JS、CSS、图片)
- `drawify-wasm/`(WASM 产物,需手动复制或配置构建工具)

> 当前配置下 WASM 产物不在 dist 中,需手动复制或调整 vite.config.ts 的 publicDir。

### 1.4 本地预览构建产物

```bash
npm run preview
```

## 2. 部署方式

### 2.1 静态文件服务器(Nginx)

```nginx
server {
    listen 80;
    server_name studio.drawify.example.com;
    root /var/www/drawify-studio;
    index index.html;

    # SPA 回退
    location / {
        try_files $uri $uri/ /index.html;
    }

    # WASM MIME 类型
    location ~ \.wasm$ {
        types { application/wasm wasm; }
    }

    # 静态资源缓存
    location /assets/ {
        expires 1y;
        add_header Cache-Control "public, immutable";
    }

    # LLM API 代理(可选,避免跨域)
    location /llm/ {
        proxy_pass https://api.openai.com/;
        proxy_set_header Host api.openai.com;
        proxy_set_header Authorization $http_authorization;
    }
}
```

### 2.2 Vercel / Netlify

1. 连接 Git 仓库
2. 构建命令:`cd studio && npm install && npm run build`
3. 输出目录:`studio/dist`
4. 环境变量:在平台配置 `VITE_LLM_*` 变量

### 2.3 Docker

```dockerfile
FROM node:18-alpine AS builder
WORKDIR /app
COPY package*.json ./
RUN npm ci
COPY . .
RUN npm run build

FROM nginx:alpine
COPY --from=builder /app/dist /usr/share/nginx/html
COPY nginx.conf /etc/nginx/conf.d/default.conf
EXPOSE 80
CMD ["nginx", "-g", "daemon off;"]
```

构建与运行:

```bash
docker build -t drawify-studio .
docker run -p 8080:80 drawify-studio
```

### 2.4 GitHub Pages

1. 在仓库 Settings → Pages
2. Source: GitHub Actions
3. 添加 `.github/workflows/deploy-studio.yml`:

```yaml
name: Deploy Studio
on:
  push:
    branches: [main]
    paths: ['studio/**']
jobs:
  build-deploy:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
          target: wasm32-unknown-unknown
      - run: cargo install wasm-pack
      - name: Build WASM
        run: |
          cd crates/drawify-wasm
          wasm-pack build --target web --release --out-dir ../../studio/drawify-wasm
      - uses: actions/setup-node@v4
        with:
          node-version: 18
      - name: Build Studio
        run: |
          cd studio
          npm install
          npm run build
      - uses: peaceiris/actions-gh-pages@v3
        with:
          github_token: ${{ secrets.GITHUB_TOKEN }}
          publish_dir: studio/dist
```

## 3. 环境变量配置

### 3.1 构建时变量

Vite 环境变量在构建时注入,前缀必须为 `VITE_`:

| 变量 | 必填 | 说明 |
|------|------|------|
| VITE_LLM_PROVIDER | 否 | 默认 openai |
| VITE_LLM_API_KEY | 否 | 可由用户在前端设置覆盖 |
| VITE_LLM_MODEL | 否 | 默认 gpt-4o |
| VITE_LLM_BASE_URL | 否 | 默认 OpenAI 端点 |
| VITE_AGENT_MAX_ITERATIONS | 否 | 默认 10 |

### 3.2 运行时配置

用户可在前端设置面板修改 LLM 配置,覆盖构建时默认值,存储于 localStorage。

### 3.3 生产环境建议

- 不在构建产物中硬编码 API Key
- 提供前端设置入口,用户自行填入
- 如需代理 LLM 请求,配置 Nginx 反向代理(见 2.1)

## 4. WASM 产物管理

### 4.1 产物位置

WASM 产物位于 `studio/drawify-wasm/`,由 wasm-pack 生成,不入版本控制。

### 4.2 CI/CD 构建

在 CI 中自动构建 WASM:

```bash
# 安装 wasm-pack
cargo install wasm-pack

# 构建
cd crates/drawify-wasm
wasm-pack build --target web --release --out-dir ../../studio/drawify-wasm
```

### 4.3 版本对齐

WASM 产物版本由 drawify-wasm 的 Cargo.toml 决定。Studio 通过 `wasm.version()` 获取版本号并显示在顶栏。

## 5. 监控与日志

### 5.1 前端日志

- 开发环境:console.log/warn
- 生产环境:移除 console.log,保留 console.error
- Agent 执行步骤通过 `onStep` 回调,可接入监控

### 5.2 错误上报(可选)

可接入 Sentry 等错误监控:

```typescript
// main.tsx
import * as Sentry from '@sentry/react';
Sentry.init({ dsn: 'YOUR_DSN' });
```

### 5.3 性能监控

- WASM 加载耗时
- Agent 单次迭代耗时
- 渲染耗时

可通过 `performance.now()` 在关键节点打点。

## 6. 回滚策略

### 6.1 静态部署回滚

- 保留前 N 个版本的构建产物
- Nginx 切换 root 目录即可回滚
- Docker 回滚到上一个镜像 tag

### 6.2 WASM 版本回滚

- WASM 产物与前端版本强绑定
- 回滚前端时需同步回滚 WASM 产物
- 建议在 CI 中将 WASM 产物与前端一起打包

## 7. 常见问题

### Q: WASM 加载失败?

A: 检查 `studio/drawify-wasm/` 目录是否存在产物,以及服务器是否正确配置 `.wasm` 的 MIME 类型为 `application/wasm`。

### Q: LLM 请求跨域?

A: 浏览器直接请求 LLM API 可能跨域。解决方案:
1. 配置 Nginx 反向代理(见 2.1)
2. 使用支持 CORS 的 LLM Provider(如 Ollama 本地部署)

### Q: Agent 一直循环不结束?

A: 检查 `VITE_AGENT_MAX_ITERATIONS` 配置,默认 10 次。LLM 可能因 Prompt 不清晰导致循环,优化 System Prompt。

### Q: apply_patch 报错"不支持"?

A: 当前 WASM 产物未包含 `apply_patch` 绑定。需在 drawify-wasm crate 新增导出后重新构建(见 [tasks.md](tasks.md) P1 阶段)。
