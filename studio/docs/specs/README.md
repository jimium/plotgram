# 接口规范

Drawify Studio 前端模块的接口与协议规范。

> 注意：Drawify DSL 语言规范、AST 规范、Pipeline 规范等核心规范在项目根目录 `docs/specs/` 中，此处仅存放 Studio 前端自身的接口规范。

## 规范范围

| 规范 | 说明 |
|------|------|
| Agent Tool 接口 | Agent 可用 Tool 的 schema 与执行器协议 |
| LLM 客户端接口 | LLMClient / LLMStreamChunk 等前端-LLM 通信协议 |
| WASM 桥接层接口 | Studio 与 drawify-wasm 的 TypeScript 接口定义 |
| 配置存储协议 | localStorage 配置项的键名与格式约定 |

## 命名规范

- 文件名格式：`{module-name}-spec.md`
- 每个规范包含：接口签名、数据结构、约束条件、变更记录
