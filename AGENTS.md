# AGENTS.md

本仓库所有 agent（AI 助手与人类协作者）必须遵守的**现行**约束。  
历史豁免与设计期叙事见 [`docs/新架构/23`](docs/新架构/23-Atlas分阶段推进方案-2026-07.md) / git；勿再按「门禁全关」理解本仓库。

## 1. yFiles 第一性原理（布局 / 路由设计）

plotgram 由 yFiles 经验孵化：轻量 DSL 画图引擎。设计布局与边路由时，**默认采用 yFiles 沉淀的理念与纪律**；**不**复刻全量复杂布局栈，只取最小完备子集。

硬约束（Hier / 正交主路径；其它图种取适用子集）：

- **每个几何自由度有且只有一个写者**（single writer per degree of freedom）。
- **落笔相零新决策**：Ink / sanitize / 事后修只展开上游已决事项；若需要「发明」端口位置、肘点拓扑、track 坐标等，说明上游决策缺位——先上提写者，禁止在落笔打补丁假装修好。
- **下游不得推翻上游，只得展开上游**。判断句：修法若是「在 Ink 再加一个特判」，先问「这个自由度的写者应该是谁」。

最小闭环（详见 [`yFiles 第一性原理与写权纪律`](docs/总结经验/yFiles第一性原理与写权纪律-2026-07.md)、[`22`](docs/新架构/22-Atlas下一代布局与路由架构-总纲-2026-07.md) B1–B6）：拓扑 → 端口 → 度量 → 落笔 → 标注。读什么、改哪里见 §8。

创新模式下可推翻管线或换生成器，但**不豁免**本条单写者与落笔零决策。

## 2. 无向后兼容

项目尚未对外发布。可自由重命名、删除、重构公共 API；直接删旧代码，不要留 deprecated 转发层。

## 3. 布局与边路由：确定性迭代

**不得**依赖 `HashMap` 的 key 排序驱动迭代。需要稳定序时用显式排序或 `IndexMap` / `BTreeMap`，并在排序键上保证确定性。

## 4. WASM 禁裸 `std::time`

`plotgram-core` 会编到 WASM。在 `crates/plotgram-core/src` 内禁止 `std::time::{Instant, SystemTime}`。计时与性能日志统一走 [`layout/perf.rs`](crates/plotgram-core/src/layout/perf.rs) 的 `Instant` 与 `perf_log!`。

## 5. 日常验证

- **禁默认 `--release`**：日常用 `cargo check` / `cargo test` / `cargo run -p plotgram-cli`（debug）。`--release` 仅用于性能测量与门禁脚本（脚本内自行构建）。
- **勿信陈旧 binary**：优先 `cargo run -p plotgram-cli`，勿直接跑 `./target/release/plotgram`。
- 现行门禁：[`gate-switch.sh`](benchmarks/scripts/gate-switch.sh) `GATES_DEFAULT=on`。

## 6. 质量棘轮与禁止图名特判

- 日常「无退化」看 [`benchmarks/`](benchmarks/) **product-gate**（[`product-regression-set.txt`](benchmarks/sets/product-regression-set.txt)），不是全量 showcase / stress。stress 质量默认 WARN；穿组 / `det=true` 全角色仍硬。
- 抬基线 `note` 须带角色（`raise product:` / `raise stress (expected):`）。
- **禁止图名特判**；不可为压 stress 数字加图名分支。

## 7. 创新模式（算法 / 架构级重写）

仅限推翻管线或更换生成器一类重写；日常 bug / 调参仍走 §6 棘轮。

开启前写清：**目标维度**、**可接受的临时退化范围**、**退出判据**。评判用帕累托（目标显著改善，其它关键维无不可接受退化）；提交须显式抬基线。  
§1 / §3 / §4 / 禁图名特判 / `cargo run` 验真——创新模式下**不豁免**。

## 8. 改布局 / 路由时读什么

Hier 生产路径是 **Atlas 三相**（组合 → 度量 → Ink），不要默认按旧正交 coordinator 时序改。设计尺子见 §1。

1. [`yFiles 第一性原理与写权纪律`](docs/总结经验/yFiles第一性原理与写权纪律-2026-07.md) — 设计尺子与写权（§1 细则）  
2. [`布局与路由核心手册`](docs/总结经验/布局与路由核心手册-2026-07.md) — 踩坑与验证红线  
3. [`docs/新架构/README`](docs/新架构/README.md) — Atlas 文档入口与阶段性记债

`docs/已经实现的方案/` 可参考，不代表最终实现。lint warning 只是参考，不为消 warning 牺牲性能或引入图名特判。

## 9. 单元测试

- **表驱动**：多 case 合并为一个 `#[test]` + 循环。  
- **断言可观测输出**（坐标 / 路径 / SVG），不断言内部计数器。  
- 布局坐标优先 `insta::assert_json_snapshot!`。  
- 确定性等属性保留最高层（pipeline / shadow）即可，勿三层各测一遍。
