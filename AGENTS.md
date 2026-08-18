# AGENTS.md

本仓库所有 agent（AI 助手与人类协作者）必须遵守的**现行**约束。  
当前阶段：**重建 tautcore**（新 crate：`tautcore-model` / `engine` / `render` / `cli`；旧实现在 `crates/v1/`，只读参考）。

## 1. 设计尺子（布局 / 路由）

tautcore 由 yFiles 经验孵化：轻量 DSL 画图引擎。设计布局与边路由时，**默认采用 yFiles 沉淀的理念与纪律**；**不**复刻全量复杂布局栈。

硬约束（Hier / 正交主路径；其它图种取适用子集）：

- **每个几何自由度有且只有一个写者**
- **落笔相零新决策**：Ink / 事后修只展开上游；若需要「发明」端口、肘点、track 等，先上提写者
- **下游不得推翻上游，只得展开上游**。判断句：修法若是「再加一个特判」，先问「这个自由度的写者应该是谁」
- **声明表堆谓词先问目标函数**：某自由度靠认领表 / 硬等式 / 再加排除条件维持时，优先改成**可最小化的 `J` + 少量硬约束 + typed 权重**；调不通先改 `J` / 权重，不加第三趟 ad-hoc。细则：[`write-authority.md` §2.2](docs/design/layout/write-authority.md)

细则（单写者 / 落笔）：[`docs/design/layout/write-authority.md`](docs/design/layout/write-authority.md)。  
重建可推翻管线或换生成器，**不豁免**本条。

## 2. 工程红线

- **无向后兼容**：可自由重命名、删除、重构；直接删旧代码，不留 deprecated 转发层。
- **确定性**：布局 / 路由迭代不得依赖 `HashMap` key 序；需稳定序时用显式排序或 `IndexMap` / `BTreeMap`。
- **禁止图名特判**：引擎不按 diagram type 分支；图种差异只经 profile 展开进算法参数（见 [`ADR-001`](docs/design/adr/001-diagram-type-not-in-engine.md)）。
- **验真**：日常 `cargo check` / `cargo test` / `cargo run -p tautcore-cli`（debug）。`--release` 仅用于性能测量。优先 `cargo run`，勿信陈旧 binary。
- **WASM 计时**：会编到 WASM 的布局 / 引擎代码内禁止裸 `std::time::{Instant, SystemTime}`；统一走项目的 perf 抽象（勿钉死 v1 路径）。

## 3. 读什么

1. [`docs/design/`](docs/design/) — 现行设计与 ADR；布局内核见 [`docs/design/layout/`](docs/design/layout/)  
2. [`docs/specs/dsl/dsl-spec.md`](docs/specs/dsl/dsl-spec.md) — DSL 语法契约 + 属性注册表（§14）；[`archetype-spec.md`](docs/specs/archetype-spec.md) — archetype / CSV  
3. [`docs/specs/style-sheet-spec.md`](docs/specs/style-sheet-spec.md) — 主题与视觉词表  
4. [`docs/reference/yFiles-layouts-and-routing.md`](docs/reference/yFiles-layouts-and-routing.md) — yFiles 产品能力参考  
5. [`docs/archive/`](docs/archive/) — 历史设计（**只读**，不驱动实现）

## 4. 单元测试

- 表驱动：多 case 合并为一个 `#[test]` + 循环  
- 断言可观测输出（坐标 / 路径 / SVG），不断言内部计数器  
- 布局坐标优先 `insta::assert_json_snapshot!`  
- 确定性等属性保留最高层即可，勿多层各测一遍
