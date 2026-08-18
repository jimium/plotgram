# Tautcore Showcase

经典示例集 + 视觉监视画廊 + 质量门禁数据源。

> 重建方案见 [`docs/design/showcase/showcase-redesign-2026-08.md`](../docs/design/showcase/showcase-redesign-2026-08.md)。本文是**现行目录与命名契约**。

## 三层模型

```text
showcase/
  {layout}/                 ← 全局统一：引擎注册名（ADR-001）
    [{facet}/]              ← 可选：该内核自己的能力里程碑分桶
      {role}.{slug}.taut     ← 全局统一：smoke / product / demo / stress / mech
```

| 层 | 谁定义 | 全库是否同一套 |
|----|--------|----------------|
| **layout** | 必保内核表 | ✅ 统一（`hierarchical` / `tree` / `sequence` / `circular`） |
| **facet** | **各内核自定** | ❌ 不统一词汇；没有则文件直接落在 `{layout}/` |
| **role** | 本 README | ✅ 统一 |

夹具只保证：`discover` 扫 `{layout}/**/*.taut`；画廊按 `layout` →（可选）`facet` → `role` 过滤。空 facet 目录可占位（如 `partition/`），不进门禁。

### 不变式

1. **目录名 = 布局内核注册名**（ADR-001），不是图种。禁止再建 `architecture/`、`mindmap/`、`flowchart/` 等图种目录。
2. **禁止**规定「每个 layout 都必须有 flat/group/partition」。facet 词汇写在该 layout 的说明里（见下）。
3. **角色前缀全局不变**；门禁按 role，路径带 `layout/`（及可选 facet）。
4. 假能力不进错桶：用 group 冒充的「泳道」进 hier 的 `group/`；真 `PartitionGrid` 才进 `partition/`。
5. 顶层除 `_` 前缀与已知非样例项（`index.html`、`render.sh`、`scripts/`、`assets/`）外，**每一个目录都是布局族**；`scripts/discover.py` 自动发现，无第三份注册表。
6. `_backup/` 与 `_out/` 永不被自动发现扫入。

### 路径解析

```text
hierarchical/flat/smoke.decision-loop.taut
└ layout ─┘ └facet┘ └role┘ └──── slug ────┘

hierarchical/group-weak/product.cloud-native.taut
tree/single-layer/smoke.root-branches.taut   # 现行；facet = placer 族
sequence/smoke.ping-pong.taut                # 现行；无 facet
```

- `layout` = 路径第一段  
- `facet` = 第二段（若存在且不是文件名）  
- `role` = 文件名第一段（`.` 前）  

manifest 写入 `layout` / `facet`（可 null）/ `role`；SVG 镜像源路径：`_out/{path 去 .taut}.svg`。

---

## 各内核的 facet 词汇

### hierarchical（一期）

| facet | 含义 | 状态 |
|-------|------|------|
| `flat` | 无 group、无 PartitionGrid | 现行主路径 |
| `group` | 有 group（含用 group 表达分区的旧「泳道」样例） | Weak 子集；StrongMacro 未实现 |
| `partition` | 真 PartitionGrid（`partition { }` + `cell_col`/`cell_row`） | **空目录占位**；等 parse + 引擎 M5 |

### tree（facet = placer 族）

对齐 yFiles：组织图 / mindmap / dendrogram 是同一 `layout: tree` 换 SubtreePlacer，**禁止**再建 `mindmap/` 图种目录。细则见 [`tree/README.md`](tree/README.md)。

| facet | 含义 | 状态 |
|-------|------|------|
| `single-layer` | 默认 `SingleLayerSubtreePlacer`（子水平排、父居中） | **M1 Buchheim** |
| `single-split-layered` | `SingleSplit` + 两侧 `LevelAligned` 对向旋转；**mindmap 主路径** | **M2** |
| `left-right` | `LeftRight` / `Bus`：竖直总线 | **M2**；文件树 / 多直属 |
| `double-layer` | 子分两行交错 + 水平总线 | **M3** |
| `dendrogram` | 叶底对齐 | **M3** |
| `assistant` | 标记子走左右总线，其余在下 | **M4** |
| `compact` | 有界策略搜索，接近目标长宽比 | **M4** |
| `aspect-ratio` | 按目标宽高比切行/列，根在左上角 | **M4** |
| `radial` | `placer: radial`：同深度同心圆 | **M5** |
| `balloon` | `placer: balloon`：子树圆盘绕父 | **M5** |
| `mixed` | 节点级 `subtree_placer`（默认分层 + 局部总线） | 收口 |

### sequence

| 内核 | facet | 说明 |
|------|-------|------|
| sequence | （无；文件直接落在 `sequence/`） | M4 生命线 + 消息 + 组合片段框 |

### 其它内核（毕业时再定）

| 内核 | 建议 facet | 说明 |
|------|------------|------|
| circular | `cycle` / `bcc` / `custom` | 单环 vs BCC vs 作者 `circle:`；**M3** |

---

## 目录结构

```
showcase/
├── hierarchical/
│   ├── flat/                 # 无 group
│   ├── group/                # 有 group
│   ├── partition/            # PartitionGrid 占位（可空）
│   └── README.md             # hier facet 说明（可选短注）
├── tree/                     # 现行种子；facet = placer 族
│   └── single-layer/         # 默认 SingleLayer；M1 起对齐 RT 美学
├── sequence/                 # 现行；无 facet（M4 生命线 + 消息 + 片段）
├── circular/                 # M3：cycle/ 单环 + bcc/ 多环 + custom/ 作者分区
│   ├── cycle/
│   ├── bcc/
│   └── custom/
├── _backup/                  # 未支持内核的旧 DSL；永不进渲染 / 门禁
├── _out/                     # SVG + manifest.json（gitignore；render.sh 派生）
├── scripts/                  # 发现 / 增量 / 写 manifest（仅 Python 3 标准库）
├── index.html                # 监视画廊
├── render.sh
└── assets/
```

### `_backup/` 毕业规程

见 [`showcase-redesign-2026-08.md` §4.2](../docs/design/showcase/showcase-redesign-2026-08.md)。内核可出图后：建 `showcase/{layout}/` → 重写为现行 DSL → 按该核 facet 分桶 → smoke 稳定后再入 CI。

`_backup/state/`：按 profile 拆入 `hierarchical/` 或 `circular/`，**不**建 `showcase/state/`。

---

## 角色前缀

| 前缀 | 角色 | 观感 | 门禁默认 |
|------|------|------|----------|
| `smoke.` | smoke | 必须干净 | 冒烟；正确性 + 质量硬 |
| `product.` | product | **必须好看** | **精选进质量硬棘轮** |
| `demo.` | demo | 好看优先 | 观测 / 可债 |
| `stress.` | stress | 可妥协 | 正确性硬；质量默认 WARN |
| `mech.` | mech | 不追美观 | 机制断言 / 专项集 |

---

## 日常用法

```bash
./showcase/render.sh                 # 增量渲染 → _out/ + manifest
./showcase/render.sh --serve         # 渲染后起静态服务（默认 4173）
./showcase/render.sh --layout hierarchical
./showcase/render.sh --force
./showcase/render.sh --debug
```

画廊需经 `render.sh --serve` 或任意静态服务器打开（`file://` 下 fetch manifest 会失败）。

---

## 门禁清单

路径相对 `showcase/`，**必须含 layout**（及 facet，若有）：

```text
hierarchical/flat/smoke.decision-loop.taut
hierarchical/group-weak/product.cloud-native.taut
```

| 清单 | 角色 |
|------|------|
| `benchmarks/sets/smoke-set.txt` | smoke |
| `benchmarks/sets/product-regression-set.txt` | product |
| `benchmarks/sets/stress-probe-set.txt` | stress |
| `benchmarks/sets/demo-observe-set.txt` | demo |
| `benchmarks/sets/mech-set.txt` | mech |

---

## DSL 语法（v2.6）

见 [`docs/specs/dsl/dsl-spec.md`](../docs/specs/dsl/dsl-spec.md)。

```tautcore
diagram {
    profile: flowchart
    title: "..."
    layout: hierarchical { direction: top-to-bottom }

    node login { label: "登录", archetype: start }
    group auth {
        label: "认证服务"
        node api { label: "API", archetype: service }
    }
    login -> api "提交"
}
```

---

## 代表样例

| 主题 | 文件 | facet |
|------|------|-------|
| 冒烟决策环 | `hierarchical/flat/smoke.decision-loop.taut` | flat |
| 扁平 REST | `hierarchical/flat/product.flat-rest-api.taut` | flat |
| 三层架构 | `hierarchical/flat/product.three-tier.taut` | flat |
| 云原生 | `hierarchical/group-weak/product.cloud-native.taut` | group-weak |
| 跨部门泳道 | `hierarchical/group-strong-macro/product.swimlane-order-process.taut` | group-strong-macro |
| yFiles 管道压测 | `hierarchical/flat/stress.layout-stress-yfiles-pipeline.taut` | flat |
| D2 对照 | `hierarchical/group-weak/product.d2-cell-tower-network.taut` | group-weak |
| 树冒烟三分支 | `tree/single-layer/smoke.root-branches.taut` | single-layer |
| 组织架构 | `tree/single-layer/product.org-chart.taut` | single-layer |
