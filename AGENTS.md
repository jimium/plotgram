# AGENTS.md

本文件记录本仓库中所有 agent(包括 AI 助手与人类协作者)必须遵守的项目规则。

## 1. 无向后兼容约束

本项目尚未对外发布,**不需要考虑向后兼容**。

- 可以自由重命名、删除、重构公共 API。
- 可以删除过时的模块、函数、类型,无需保留 deprecated 标记或兼容包装器。
- 重构时直接删除旧代码,不要保留"向后兼容的转发层"。

## 2. 布局与边路由的确定性迭代

实现布局算法、边路由算法时,**不得依赖 HashMap 的 key 排序来驱动迭代顺序**。

- HashMap 的迭代顺序不稳定,同一输入多次渲染可能产生不同结果,导致图形抖动。
- 需要稳定顺序时,应使用显式排序(如按 id、拓扑序、插入序)或 `IndexMap`/`BTreeMap` 等有序容器,并在排序键上保证确定性。

## 3. docs/已经实现的方案 文件夹
这个文件夹下存放已经实现的方案。可用来参考，但不一定代表代码的最终实现。

## 4. 算法优化中 lint 的使用原则

优化布局/路由算法时，**lint 结果只是参考，没必要 100% 消除所有 warning**。

- lint 规则（尤其是 Warning 级别）用于提示潜在质量退化，但算法性能、简洁性和可维护性同样重要。
- 在性能与 lint 干净度发生冲突时，优先保证算法性能与代码简洁性。
- 不得为了消除 lint warning 而引入过度复杂的逻辑或显著降低性能。
- 对于 edge bundling 等启发式算法，少量 warning 是可接受的，只要核心效果（ink 节省、视觉清晰度）达标。

## Cursor Cloud specific instructions

本仓库是 Drawify：一个 Rust workspace（`crates/`）加两个浏览器前端（`playground/`、`studio/`）。
启动脚本（update script）已自动完成：将 Rust 默认工具链切到 stable，并为 playground / studio 执行 `npm install`。

### 关键非显然事项（gotchas）

- **Rust 工具链版本**：依赖树中的 `indexmap 2.14+` 需要 edition 2024，必须用 **Rust ≥ 1.85（当前为 stable 1.96）**。基础镜像自带的 1.83 会报 `feature 'edition2024' is required` 而无法编译。update script 会执行 `rustup default stable`；若编译报该错误，先确认 `rustc --version`。
- **WASM 产物不入版本库且不由 update script 构建**。`playground/drawify-wasm/` 和 `studio/drawify-wasm/` 由 `wasm-pack` 生成（已 gitignore）。两个前端 dev server 启动前必须先存在这些产物，否则页面渲染会失败。重新构建命令：
  - `cd crates/drawify-wasm && wasm-pack build --target web --out-dir ../../playground/drawify-wasm`
  - `cd crates/drawify-wasm && wasm-pack build --target web --out-dir ../../studio/drawify-wasm`
  - 修改了 `drawify-core`/`drawify-wasm` 的 Rust 代码后，必须重新执行上面的 `wasm-pack build`，前端的热更新**不会**自动重编 WASM。
  - `wasm-pack` 是构建工具（通过 `cargo install wasm-pack` 安装），若 `command -v wasm-pack` 为空需先安装。`playground/start.sh` 封装了「构建 WASM + 起 dev server」。
- **DSL 语法以 `showcase/*.dfy` 为准，README 的快速示例已过时**。方向用 `config { direction: left-to-right }`，不是 `layout:`。验证语法时优先复制 `showcase/` 下的真实示例。
- **`drawify-server` 的 `/render` 对合法输入直接返回原始 SVG（200，非 JSON）**；只有出错时才返回 JSON 诊断。`/validate` 始终返回 JSON。
- **`studio` 的 `npm run lint` 当前会失败**：`studio/eslint.config.js` 引用了未在 `package.json` 声明的 `typescript-eslint`。这是仓库自身缺失依赖，非环境问题；`studio` 的 `npm run test`（vitest）、`npm run typecheck`、`npm run dev` 均正常。`playground` 的 lint 正常。
- **`studio` 的 LLM/Agent 核心功能需要 LLM API Key**（见 `studio/.env.example`，复制为 `.env.local`）。无 Key 时 dev server 仍能启动、WASM 渲染仍可用，但自然语言驱动的对话流程无法工作。

### 各服务运行方式

- 工作区构建/测试/lint：`cargo build --workspace` / `cargo test --workspace` / `cargo clippy --workspace`（根目录）。
- CLI：`cargo run -p drawify-cli -- render <file.dfy> -f svg -o out.svg`（须在仓库根目录运行，路径相对根目录）。
- HTTP server：`cargo run -p drawify-server`，监听 `0.0.0.0:6080`（可用 `DRAWIFY_SERVER_ADDR` 覆盖）。
- Playground dev server：`cd playground && npm run dev` → http://localhost:3000 （需先构建 WASM）。
- Studio dev server：`cd studio && npm run dev` → http://localhost:3100 （需先构建 WASM）。
