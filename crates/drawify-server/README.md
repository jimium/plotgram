# drawify-server

Drawify 的 HTTP 服务，提供远程校验与渲染能力。

## 快速启动

```bash
cargo run -p drawify-server
```

默认监听 `http://0.0.0.0:6080`。

## 接口文档

完整使用说明见：[docs/architecture/drawify-server-api.md](../../docs/architecture/drawify-server-api.md)

| 方法 | 路径 | 说明 |
|------|------|------|
| `GET` | `/health` | 健康检查 |
| `POST` | `/validate` | 语法与语义校验 |
| `POST` | `/render` | 渲染（成功时直接返回图片/SVG 等） |
