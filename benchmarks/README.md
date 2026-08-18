# benchmarks

布局 / 边路由的**回归门禁数据与工具**（角色感知）。

```
benchmarks/
├── README.md / README.html   # 说明（本文件 + 可读版）
├── sets/                     # 门禁样例清单
├── baselines/                # 快照 JSON/MD（latest + YYYY-MM-DD-HHMMSS）
├── scripts/                  # snapshot / compare / serve-viewer
├── viewer/                   # 基线趋势 UI
├── snapshot.sh               # → scripts/ 薄包装
├── compare.sh
└── serve-viewer.py
```

完整说明（含流程 / 指标图解）见可读版：

- 文件：[`README.html`](./README.html)
- 本地服务：`./benchmarks/serve-viewer.py` → http://127.0.0.1:8765/readme

**产品规定（共线 / 合流验收语言）**：  
[`docs/architecture/方案计划/collinear-and-arrow-merge-comparison.md`](../docs/方案计划/collinear-and-arrow-merge-comparison.md) §2  
（Allowed / NeedsSeparation / Degraded；不以「共线计数归零」为成功标准。）

**角色分层重构方案**：[`docs/architecture/重构方案/showcase-基线分层重构-2026-07.md`](../docs/architecture/重构方案/showcase-基线分层重构-2026-07.md)

## 门禁集（按角色分集）

| 清单 | 角色 | 用途 | compare 行为 |
|------|------|------|--------------|
| [`sets/product-regression-set.txt`](./sets/product-regression-set.txt) | `product` | 日常质量硬门禁（精选） | 正确性 + 质量 **硬 FAIL** |
| [`sets/stress-probe-set.txt`](./sets/stress-probe-set.txt) | `stress` | 正确性硬 + 质量观测 | 正确性硬；质量 WARN（`--strict-stress` 改硬） |
| [`sets/mech-set.txt`](./sets/mech-set.txt) | `mech` + 双用途 `product` | 机制探针 / 拥堵校准 | 默认不门禁（机制断言另跑） |
| [`sets/demo-observe-set.txt`](./sets/demo-observe-set.txt) | `demo` | 观测 / 可债 | 正确性硬；质量 WARN |

日常宣称「无退化」默认只引用 **product-gate**。文件名首段即角色（`{role}.{slug}.taut`），无需 manifest overrides。

## 速查

```bash
# 采快照（默认 product + stress → baselines/YYYY-MM-DD-HHMMSS.{json,md} 与 latest）
./benchmarks/snapshot.sh

# 同日多次时加 tag，便于区分
./benchmarks/snapshot.sh --tag after-stub-fix

# 只采 product 门禁
./benchmarks/snapshot.sh --set benchmarks/sets/product-regression-set.txt

# 加入 demo 观测集
./benchmarks/snapshot.sh \
  --set benchmarks/sets/product-regression-set.txt \
  --set benchmarks/sets/stress-probe-set.txt \
  --set benchmarks/sets/demo-observe-set.txt

# 对比门禁（默认：product 硬 / stress 软 / demo 软 / mech 不门禁）
./benchmarks/compare.sh \
  benchmarks/baselines/latest.json \
  path/to/new-snapshot.json

# 显式把 stress 质量也走硬 fail（探针严格模式）
./benchmarks/compare.sh --strict-stress baseline.json current.json

# 全部质量轨转 WARN（显式债，仍 exit 0）
./benchmarks/compare.sh --allow-quality-debt baseline.json current.json

# 可视化基线变化 + 打开文档
./benchmarks/serve-viewer.py
```

当前门禁指针：[`baselines/latest.json`](./baselines/latest.json)。

### 快照命名与 `--tag`

每次 `snapshot.sh` 写出一份**时间戳归档**，并同步覆盖 `latest`：

| 产物 | 说明 |
|------|------|
| `baselines/YYYY-MM-DD-HHMMSS.{json,md}` | 默认归档名（精确到秒，同日多次不互相覆盖） |
| `baselines/YYYY-MM-DD-HHMMSS-<tag>.{json,md}` | 传入 `--tag` 时追加可读后缀 |
| `baselines/latest.{json,md}` | 始终指向**最近一次**采集 |

```bash
./benchmarks/snapshot.sh                         # → 2026-07-20-102530.*
./benchmarks/snapshot.sh --tag after-stub-fix   # → 2026-07-20-102530-after-stub-fix.*
```

- `--tag` **可选**；仅允许字母、数字、`.`、`_`、`-`。
- JSON 内 `date` 字段与归档 stamp 一致，便于 viewer / 同日多次排序。
- 中间实验不必全进仓库；对照至少保留 `latest` + 上一版有意义快照。

## 角色感知分轨

每条样本在 JSON 中带 `role` 字段（取文件名第一段：`product` / `stress` / `demo` / `mech` / `smoke`）。`compare.sh` 按角色决定质量轨是否挡合并：

| 轨 | 检查 | product / smoke | demo | stress | mech |
|----|------|-----------------|------|--------|------|
| 正确性（硬） | `edge_crosses_group_interior` 不升；`det=true` | FAIL | FAIL | FAIL | FAIL |
| 质量（默认真） | `exact_sev` / `tight_sev`；lint through/trunk/err；`ortho.degraded_count`；perf；`node_fp` | **FAIL** | WARN / 可债 | WARN（`--strict-stress` 改硬） | 不门禁 |
| 观测（WARN） | `allowed_share_len` 可升；若 allowed↑ 且 exact 未降 → 提示抽检误标 Allowed | 同现网 | 同现网 | 同现网 | — |

抬基线 `note` 强制带角色（手册 §1）：

```text
raise product: …原因…；残余: product.foo
raise stress (expected): …探针可接受…；残余: stress.layout-stress-nested
```

## 与创新模式对齐（`AGENTS.md` §7；设计尺子见 §1）

| 模式 | product-gate | stress-probe |
|------|--------------|--------------|
| 日常修复（棘轮） | 不劣化 | 质量可债；正确性不劣化 |
| 算法级重写（帕累托） | 目标维度优先看本集 | 允许更大临时质量退化，退出时显式抬基线 + 列残余 |
