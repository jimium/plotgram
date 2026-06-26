# Drawify Server API 使用说明

`drawify-server` 是 Drawify 的 HTTP 服务，供 Agent、CI、IDE 插件等远程调用解析、校验与渲染能力，无需安装 CLI。

默认监听 **`http://0.0.0.0:6080`**。

---

## 启动服务

```bash
# 项目根目录
cargo run -p drawify-server
```

### 环境变量

| 变量 | 默认值 | 说明 |
|------|--------|------|
| `DRAWIFY_SERVER_ADDR` | `0.0.0.0:6080` | 监听地址 |
| `DRAWIFY_FONTS_DIR` | （未设置时使用 `cwd/fonts/`） | PNG/WebP 渲染用的 CJK 字体目录 |

```bash
DRAWIFY_SERVER_ADDR=127.0.0.1:6080 \
DRAWIFY_FONTS_DIR=/path/to/fonts \
cargo run -p drawify-server
```

---

## 接口总览

| 方法 | 路径 | 说明 |
|------|------|------|
| `GET` | `/health` | 健康检查 |
| `POST` | `/validate` | 语法与语义校验（仅诊断，不渲染） |
| `POST` | `/render` | 校验通过后渲染；成功时直接返回产物 |

所有 `POST` 请求体均为 `Content-Type: application/json`。

---

## GET /health

健康检查，用于探活与负载均衡。

**响应**

- 状态码：`200`
- Body：纯文本 `ok`

```bash
curl http://localhost:6080/health
```

---

## POST /validate

对 Drawify 源码做完整校验：词法/语法解析 + 语义验证。

适合「只检查、不渲染」的场景，例如 Agent 在提交渲染前先确认代码是否合法。

### 请求体

```json
{
  "source": "diagram flowchart { ... }"
}
```

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `source` | string | 是 | Drawify 源码全文 |

### 成功响应

- 状态码：`200`
- `Content-Type: application/json`

```json
{
  "valid": true,
  "check": {
    "passed": true,
    "errors": [],
    "warnings": []
  }
}
```

### 校验失败响应

- 状态码：`200`（请求本身合法，只是源码有问题）
- `valid: false`，`check.errors` 包含结构化错误列表

```json
{
  "valid": false,
  "check": {
    "passed": false,
    "errors": [
      {
        "code": "E008",
        "severity": "error",
        "category": "parse",
        "message": "意外的 token 'end of file'，期望: '}'",
        "location": {
          "start": { "line": 1, "column": 29 },
          "end": { "line": 1, "column": 29 }
        },
        "context": {
          "unexpected": "end of file",
          "expected": ["'}'"]
        }
      }
    ],
    "warnings": []
  }
}
```

错误对象字段说明见 [错误模型](../specs/error-model.md)。

### 示例

```bash
curl -X POST http://localhost:6080/validate \
  -H 'Content-Type: application/json' \
  -d '{"source": "diagram flowchart { entity a \"A\" { type: start } }"}'
```

---

## POST /render

渲染 Drawify 图表。**渲染前会自动执行与 `/validate` 相同的校验**；只有校验通过才会返回渲染结果。

### 设计原则

- **成功时**：响应 Body 即为渲染产物本身（PNG 字节、SVG 文本等），**不套 JSON 外壳**
- **元数据**：通过自定义 HTTP Header 传递
- **失败时**：返回 JSON 结构化错误，便于 Agent 自动修正

### 请求体

```json
{
  "source": "diagram flowchart { ... }",
  "format": "svg",
  "theme_id": "builtin-light",
  "graphic_style": "excalidraw",
  "dark_mode": false
}
```

| 字段 | 类型 | 必填 | 默认值 | 说明 |
|------|------|------|--------|------|
| `source` | string | 是 | — | Drawify 源码全文 |
| `format` | string | 否 | `svg` | 输出格式，见下表 |
| `theme_id` | string | 否 | — | 主题 ID（StyleSheet）；空字符串或 `auto` 表示自动 |
| `graphic_style` | string | 否 | — | 图形风格，见下表 |
| `dark_mode` | boolean | 否 | `false` | 是否启用暗色模式 |

#### 支持的 `format`

| 值 | 成功时 Content-Type | 响应 Body |
|----|---------------------|-----------|
| `svg` | `image/svg+xml` | SVG 文本 |
| `ascii` / `text` | `text/plain; charset=utf-8` | ASCII 文本 |
| `png` | `image/png` | 原始 PNG 二进制 |
| `webp` | `image/webp` | 原始 WebP 二进制 |
| `json` | `application/json` | 图表 AST 的 JSON |

#### 支持的 `graphic_style`

仅接受规范名（与 `GraphicStyleId::as_str()` 一致），**不支持别名**。完整说明见 [`crates/drawify-core/src/graphic_style/README.md`](../../crates/drawify-core/src/graphic_style/README.md)。

| 值 | 简述 |
|----|------|
| `standard` | 标准纯色 |
| `excalidraw` | 手绘点描 |
| `cross-hatch` | 略密点描 |
| `blueprint` | 工程蓝图 |
| `spatial-clarity` | 现代UI风 |
| `neon-glow` | 霓虹光晕 |
| `stipple` | 圆点填充 |

### 成功响应

- 状态码：`200`
- Body：渲染产物（见上表）
- 附加 Header：

| Header | 说明 | 示例 |
|--------|------|------|
| `X-Drawify-Format` | 实际输出格式 | `png` |
| `X-Drawify-Valid` | 校验是否通过 | `true` |
| `X-Drawify-Warnings` | 警告列表（JSON 数组）；**仅在有警告时出现** | `[{ "code": "W001", ... }]` |

#### 保存 PNG

```bash
curl -X POST http://localhost:6080/render \
  -H 'Content-Type: application/json' \
  -d @- \
  -o diagram.png <<'EOF'
{
  "source": "diagram flowchart {\n  entity a \"开始\" { type: start }\n  entity b \"结束\" { type: end }\n  a -> b\n}",
  "format": "png"
}
EOF
```

#### 获取 SVG 并查看 Header

```bash
curl -D - -X POST http://localhost:6080/render \
  -H 'Content-Type: application/json' \
  -d '{"source":"diagram flowchart { entity a \"A\" { type: start } entity b \"B\" { type: end } a -> b }","format":"svg"}'
```

### 失败响应

校验失败、格式不支持或渲染出错时：

- 状态码：`400`
- `Content-Type: application/json`
- Header：`X-Drawify-Format`、`X-Drawify-Valid: false`；有警告时附带 `X-Drawify-Warnings`

```json
{
  "valid": false,
  "format": "png",
  "check": {
    "passed": false,
    "errors": [
      {
        "code": "E008",
        "severity": "error",
        "category": "parse",
        "message": "意外的 token 'end of file'，期望: '}'",
        "location": {
          "start": { "line": 1, "column": 29 },
          "end": { "line": 1, "column": 29 }
        }
      }
    ],
    "warnings": []
  }
}
```

---

## 客户端集成示例

### JavaScript（fetch）

```javascript
async function renderPng(source) {
  const res = await fetch('http://localhost:6080/render', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ source, format: 'png' }),
  });

  if (!res.ok) {
    const err = await res.json();
    throw new Error(err.check.errors.map(e => e.message).join('; '));
  }

  const warnings = res.headers.get('X-Drawify-Warnings');
  if (warnings) {
    console.warn('Drawify warnings:', JSON.parse(warnings));
  }

  const blob = await res.blob();
  return URL.createObjectURL(blob);
}
```

### Python

```python
import json
import urllib.request

def render_svg(source: str) -> str:
    body = json.dumps({"source": source, "format": "svg"}).encode()
    req = urllib.request.Request(
        "http://localhost:6080/render",
        data=body,
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    with urllib.request.urlopen(req) as resp:
        if resp.status != 200:
            err = json.loads(resp.read())
            raise RuntimeError(err["check"]["errors"])
        return resp.read().decode()

def validate(source: str) -> dict:
    body = json.dumps({"source": source}).encode()
    req = urllib.request.Request(
        "http://localhost:6080/validate",
        data=body,
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    with urllib.request.urlopen(req) as resp:
        return json.loads(resp.read())
```

### Agent 推荐工作流

```
1. 生成 Drawify 源码
2. POST /validate  → 若 valid=false，读取 check.errors 修正后重试
3. POST /render    → 若 400，同样读取 check.errors 修正
4. 若 200，直接消费响应 Body（图片/SVG），可选读取 X-Drawify-Warnings
```

---

## 与 WASM / CLI 的对比

| 能力 | drawify-server | drawify-wasm | drawify-cli |
|------|---------------|-------------|------------|
| 远程 HTTP 调用 | ✅ | ❌ | ❌ |
| 浏览器内运行 | ❌ | ✅ | ❌ |
| 渲染成功返回格式 | 原始产物 + Header | JSON 包 SVG | 文件/stdout |
| 结构化错误 | ✅ | 部分（字符串） | stderr 文本 |
| PNG/WebP | ✅ | ❌ | ✅ |

---

## 常见问题

### PNG 中文显示为方块？

设置字体目录，确保包含 CJK 字体（项目 `fonts/` 目录下有 Noto Sans CJK）：

```bash
DRAWIFY_FONTS_DIR=./fonts cargo run -p drawify-server
```

### 端口被占用？

```bash
DRAWIFY_SERVER_ADDR=0.0.0.0:6081 cargo run -p drawify-server
```

### 如何只校验、不渲染？

使用 `POST /validate`，响应始终为 JSON，逻辑更轻量。

### 警告在哪里看？

- `/validate`：`check.warnings`
- `/render` 成功：响应 Header `X-Drawify-Warnings`（有警告时）
- `/render` 失败：JSON body 的 `check.warnings`，以及 Header `X-Drawify-Warnings`
