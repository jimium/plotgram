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

## 5. 改造 / 修复布局与边路由时的注意事项

改造或修复布局、边路由算法前，先阅读 [`docs/总结经验/布局与路由核心手册-2026-07.md`](docs/总结经验/布局与路由核心手册-2026-07.md)。该手册合并了踩坑复盘、缺陷审计、几何契约与 V3b 等经验；后续任务至少遵守：

- **先追管线时序，再调局部启发**：确认「最终几何/label 是谁写的」（router → snap/repulse → sanitize → resolve/assign）。sanitize 会重建 label 时，避让必须作为几何冻结后的最终步骤，否则结果会被丢弃。
- **激进几何清理放在管线末尾**：router 内部 sanitize 保持保守；`merge_overshoot` 等会改变折点拓扑的逻辑，不得在节点仍可能重定位的阶段启用，否则会反馈进 space-budget 造成假回归。
- **验证产物，勿信陈旧 binary**：不要默认信任 `./target/release/plotgram`；优先 `cargo run -p plotgram-cli`。日志与源码不一致时，先核对 binary mtime / fingerprint，再怀疑控制流。
- **无退化要可量化**：对比节点坐标是否不变；用全量 showcase 的重叠严重度（而非单图观感或仅计数）判断退化；仓库既有测试失败先钉死基线，勿与本次改动混谈。
- **禁止图名特判**：从通用规则（时序、死锁启发、端口去冲突、邻近感知）出发修复；可接受合法单调台阶暂留，不可为消 warning 引入穿模或显著复杂化。

## 6. WASM 平台禁用 `std::time::{Instant, SystemTime}`

`plotgram-core` 会编译到 WASM（playground / agent-demo），在 `crates/plotgram-core/src` 内禁止裸用 `std::time::{Instant, SystemTime}`，否则会运行时 panic。计时与性能日志统一走 [`crates/plotgram-core/src/layout/perf.rs`](crates/plotgram-core/src/layout/perf.rs) 的 `crate::layout::perf::Instant` 与 `perf_log!` 宏。
