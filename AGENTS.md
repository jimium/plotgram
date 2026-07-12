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

改造或修复布局、边路由算法前，先阅读 [`docs/总结经验/布局与路由改造踩坑复盘-2026-07.md`](docs/总结经验/布局与路由改造踩坑复盘-2026-07.md)。该文档记录了 label 碰撞、overshoot Z 折、入出边共锚、近距引线等问题的真实踩坑；后续任务至少遵守：

- **先追管线时序，再调局部启发**：确认「最终几何/label 是谁写的」（router → snap/repulse → sanitize → resolve/assign）。sanitize 会重建 label 时，避让必须作为几何冻结后的最终步骤，否则结果会被丢弃。
- **激进几何清理放在管线末尾**：router 内部 sanitize 保持保守；`merge_overshoot` 等会改变折点拓扑的逻辑，不得在节点仍可能重定位的阶段启用，否则会反馈进 space-budget 造成假回归。
- **验证产物，勿信陈旧 binary**：不要默认信任 `./target/release/plotgram`；优先 `cargo run -p plotgram-cli`。日志与源码不一致时，先核对 binary mtime / fingerprint，再怀疑控制流。
- **无退化要可量化**：对比节点坐标是否不变；用全量 showcase 的重叠严重度（而非单图观感或仅计数）判断退化；仓库既有测试失败先钉死基线，勿与本次改动混谈。
- **禁止图名特判**：从通用规则（时序、死锁启发、端口去冲突、邻近感知）出发修复；可接受合法单调台阶暂留，不可为消 warning 引入穿模或显著复杂化。

## 6. WASM 平台禁用 `std::time::{Instant, SystemTime}`

`std::time::Instant` 和 `std::time::SystemTime` 在 `wasm32-unknown-unknown` 目标上**不可用**，直接调用会在运行时 panic：`RuntimeError: unreachable` + 控制台 `time not implemented on this platform`。playground / agent-demo 通过 `wasm-pack` 编译到浏览器，任何在 render / lint / parse 路径上的裸 `Instant::now()` 都会让前端白屏或 tab 崩溃。

- **必须使用 WASM-safe 的 `crate::layout::perf::Instant`**：该模块在非 wasm32 目标上 re-export `std::time::Instant`，在 wasm32 上提供一个 no-op 替身（`now()` 返回 `Instant`，`elapsed()` 返回 `Duration::ZERO`）。
  - 非 wasm32：真实计时，保留性能日志语义。
  - wasm32：返回零，不 panic。
- **新增计时点时的检查清单**：
  1. 用 `crate::layout::perf::Instant::now()`，不要用 `std::time::Instant::now()`。
  2. 如果同文件里已 `use crate::layout::perf::Instant;`，直接写 `Instant::now()` 即可；否则写全路径。
  3. PR 前在 `crates/plotgram-core/src` 全局 grep `std::time::Instant|std::time::SystemTime`，确认没有漏网（`src/bin/` 下的 bench 工具除外，它们不进 WASM）。
- **反例**：`two_phase.rs:216` 曾用 `std::time::Instant::now()` 做 EGB 阶段计时，结果在 playground 渲染 architecture 类型图时 `render_with_options` 整个崩溃。修复就是把 `std::time::Instant` 改成 `crate::layout::perf::Instant`。
- **WASM 产物同步**：改完 Rust 代码后必须 `wasm-pack build crates/plotgram-wasm --target web --release`，然后把 `crates/plotgram-wasm/pkg` 同步到 `playground/plotgram-wasm/` 和 `agent-demo/plotgram-wasm/`，否则浏览器加载的是陈旧 WASM，bug 不会消失。
- **其他 WASM 不可用的 std 功能**（同样需要平台抽象或规避）：`std::thread`、`std::fs`、`std::net`、`std::process`、`std::env::args`、`std::time::{Instant, SystemTime}`。引入新依赖前先确认其在 `wasm32-unknown-unknown` 下可编译。
