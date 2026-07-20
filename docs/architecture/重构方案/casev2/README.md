# Showcase v2 — 分层重构样例（评审稿）

> 2026-07-20（修订：文件名即角色）  
> 方案：[`showcase-基线分层重构-2026-07.md`](../showcase-基线分层重构-2026-07.md)  
> 原始 `showcase/` 保持不动；本目录供评审通过后再替换 / 迁入。

## 0. 一句话

**文件名第一个段 = 角色**；gates 只从中挑子集进门禁。  
「无退化」≡ **product-gate**；stress 正确性硬、质量软；demo 不绑架日常棘轮。

---

## 1. 命名：`{role}.{slug}.pgm`

| 前缀 | 角色 | 门禁默认 | 例子 |
|------|------|----------|------|
| `smoke.` | smoke | 冒烟；可不进硬质量门 | `flowchart/smoke.decision-loop.pgm` |
| `product.` | product | 画廊主推；**精选**进硬门禁 | `architecture/product.cloud-native.pgm` |
| `demo.` | demo | 观测 / 可债 | `architecture/demo.k8s-platform-stack.pgm` |
| `stress.` | stress | 正确性硬、质量观测 | `flowchart/stress.layout-stress-dag.pgm` |
| `mech.` | mech | 机制断言 | `flowchart/mech.constrain-sink.pgm` |

旧 `s./n./c./x.` **已废弃**（不再表示规模）。UI 解析：

```text
path.split('/').last().split('.')[0]  →  role
```

元数据：[`manifest.yaml`](./manifest.yaml)（roles 词典 + gates 指针；**无 overrides**）。

---

## 2. 用例统计

| 目录 | smoke | product | demo | stress | mech | 合计 |
|------|------:|--------:|-----:|-------:|-----:|-----:|
| flowchart/ | 2 | 6 | 9 | 1 | 1 | 19 |
| sequence/ | 2 | 3 | 8 | 1 | 0 | 14 |
| architecture/ | 1 | 11 | 12 | 1 | 1 | 26 |
| state/ | 2 | 5 | 6 | 1 | 0 | 14 |
| er/ | 2 | 4 | 3 | 1 | 0 | 10 |
| mindmap/ | 2 | 3 | 3 | 1 | 0 | 9 |
| **合计** | **11** | **32** | **41** | **6** | **2** | **92** |

### 门禁（子集）

| 清单 | 约张数 | 用途 |
|------|--------|------|
| [`gates/product-regression-set.txt`](./gates/product-regression-set.txt) | **18** | 日常质量硬门禁 |
| [`gates/stress-probe-set.txt`](./gates/stress-probe-set.txt) | **6** | 正确性硬 + 质量观测 |
| [`gates/mech-set.txt`](./gates/mech-set.txt) | **4** | 机制探针（可含 `product.*` 双用途样例） |
| [`gates/demo-observe-set.txt`](./gates/demo-observe-set.txt) | **~40** | 观测 / 可债 |

说明：mech-set 里可列出 `product.user-auth.pgm` 等——**文件名角色仍是 product**（UI badge），清单只表示「也跑机制断言」。纯机制图用 `mech.*` 前缀。

---

## 3. 相对原 showcase 的变更

| 变化 | 说明 |
|------|------|
| 前缀 | `s/n/c/x` → `smoke/product/demo/stress/mech` |
| 压力图 | `c.layout-stress-*` → `stress.layout-stress-*`（+ mindmap 一张） |
| 机制图 | `n.constrain-*` → `mech.constrain-*` |
| 业务精选 | 如 `c.cloud-native` → `product.cloud-native`（进硬门） |
| 演示大图 | 多数原 `c.*` → `demo.*` |
| 新增 | CDN / 退款回环 / 会话 / 通知 / 图书馆·医院 ER 等 |

重命名对照：[`gates/rename-map.txt`](./gates/rename-map.txt)（旧路径 → 新路径）。

---

## 4. 验证

```bash
cd docs/architecture/重构方案/casev2
cargo run -p plotgram-cli -- validate architecture/product.cdn-cache.pgm
find . -name '*.pgm' -print0 | xargs -0 -n1 cargo run -q -p plotgram-cli -- validate
```

---

## 5. 后续

1. **P0**：按本目录 `gates/*` 改造仓库 `benchmark-data/` + compare 分轨。  
2. 画廊：按文件名前缀做 badge / 默认隐藏 `stress.`/`mech.`。  
3. 评审通过后替换 `showcase/`，更新全仓路径引用。  

**不要**在未改门禁策略前，用「全体非 stress 硬棘轮」替换现网。
