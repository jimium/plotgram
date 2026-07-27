# AGENTS.md

本仓库所有 agent（AI 助手与人类协作者）必须遵守的**现行**约束。  
历史豁免与设计期叙事见 [`docs/新架构/23`](docs/新架构/23-Atlas分阶段推进方案-2026-07.md) / git；勿再按「门禁全关」理解本仓库。

## 1. 无向后兼容

项目尚未对外发布。可自由重命名、删除、重构公共 API；直接删旧代码，不要留 deprecated 转发层。

## 2. 布局与边路由：确定性迭代

**不得**依赖 `HashMap` 的 key 排序驱动迭代。需要稳定序时用显式排序或 `IndexMap` / `BTreeMap`，并在排序键上保证确定性。

## 3. WASM 禁裸 `std::time`

`plotgram-core` 会编到 WASM。在 `crates/plotgram-core/src` 内禁止 `std::time::{Instant, SystemTime}`。计时与性能日志统一走 [`layout/perf.rs`](crates/plotgram-core/src/layout/perf.rs) 的 `Instant` 与 `perf_log!`。

## 4. 日常验证

- **禁默认 `--release`**：日常用 `cargo check` / `cargo test` / `cargo run -p plotgram-cli`（debug）。`--release` 仅用于性能测量与门禁脚本（脚本内自行构建）。
- **勿信陈旧 binary**：优先 `cargo run -p plotgram-cli`，勿直接跑 `./target/release/plotgram`。
- 现行门禁：[`gate-switch.sh`](benchmarks/scripts/gate-switch.sh) `GATES_DEFAULT=on`。

## 5. 质量棘轮与禁止图名特判

- 日常「无退化」看 [`benchmarks/`](benchmarks/) **product-gate**（[`product-regression-set.txt`](benchmarks/sets/product-regression-set.txt)），不是全量 showcase / stress。stress 质量默认 WARN；穿组 / `det=true` 全角色仍硬。
- 抬基线 `note` 须带角色（`raise product:` / `raise stress (expected):`）。
- **禁止图名特判**；不可为压 stress 数字加图名分支。

## 6. 创新模式（算法 / 架构级重写）

仅限推翻管线或更换生成器一类重写；日常 bug / 调参仍走 §5 棘轮。

开启前写清：**目标维度**、**可接受的临时退化范围**、**退出判据**。评判用帕累托（目标显著改善，其它关键维无不可接受退化）；提交须显式抬基线。  
§2 / §3 / 禁图名特判 / `cargo run` 验真——创新模式下**不豁免**。

## 7. 改布局 / 路由时读什么

Hier 生产路径是 **Atlas 三相**（组合 → 度量 → Ink），不要默认按旧正交 coordinator 时序改。

1. [`布局与路由核心手册`](docs/总结经验/布局与路由核心手册-2026-07.md) — 踩坑与验证红线  
2. [`docs/新架构/README`](docs/新架构/README.md) — Atlas 文档入口  
3. [`30 实现检讨`](docs/新架构/30-Atlas实现检讨-冗余与缺失-2026-07.md) — Stage7 后下一刀（优先 M1/M3/M4，勿先 MCF / 整目录清空 ortho）

`docs/已经实现的方案/` 可参考，不代表最终实现。lint warning 只是参考，不为消 warning 牺牲性能或引入图名特判。

## 8. 单元测试

- **表驱动**：多 case 合并为一个 `#[test]` + 循环。  
- **断言可观测输出**（坐标 / 路径 / SVG），不断言内部计数器。  
- 布局坐标优先 `insta::assert_json_snapshot!`。  
- 确定性等属性保留最高层（pipeline / shadow）即可，勿三层各测一遍。
