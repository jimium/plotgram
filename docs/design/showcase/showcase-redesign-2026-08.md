# Showcase 重建方案：视觉监视 + 质量门禁

日期：2026-08。现状：showcase 仍为 v1 产物；`hierarchical/` 已放 v2.6 DSL，但画廊页 manifest 指向已删除的旧目录、SVG 为陈旧 v1 渲染、`render-all.sh` 与 `benchmarks/` 门禁全部对接 v1 CLI。本方案重建 showcase 为**视觉验证夹具 + 质量门禁数据源**。

**一期只激活 hierarchical**，但目录 / 脚本 / 画廊 / 门禁从第一天按**多布局内核**设计（对齐 [`docs/design/layout/README.md`](../layout/README.md) 必保内核：hierarchical / tree / sequence / circular）。tree 等内核落地时只「毕业」样例与增补指标，不重做夹具。

## 1. 目标与非目标

目标：

1. **快速视觉监视**：布局迭代后 30 秒内看清「哪些图变了、哪些坏了、变成什么样」。
2. **质量控制门禁**：角色分轨的指标棘轮，正确性硬、质量按角色分级，进 CI。
3. **多布局可扩展**：新布局内核可按同一约定挂入，无需改画廊架构或注册表。

非目标：

- 不做对外营销画廊（那是 website/ 的事）。
- 不重建 v1 的全量指标体系；v2 指标集从最小可用集起步，随布局开发增补。
- 不在本期把 `_backup/` 里 tree/sequence/circular/state 样例迁回主目录（等对应内核可跑再毕业）。

## 2. 现状问题（决策依据）

| # | 问题 | 证据 |
|---|------|------|
| P1 | 画廊页失效 | `index.html` manifest 指向 `architecture/` 等已删目录，0 条指向 `hierarchical/` |
| P2 | 渲染产物陈旧 | `hierarchical/*.svg`（69 个）为 v1 渲染，与新 DSL 无关 |
| P3 | 脚本与 CLI 不匹配 | `render-all.sh` 调 `plotgram render -f` / `plotgram validate`；现行 CLI 只有 `plotgram <input> [-o] [--theme]` |
| P4 | 门禁悬空 | `benchmarks/` 依赖 v1 `gate-baseline` 二进制；`sets/*.txt` 路径指向旧目录 |
| P5 | 多布局资产闲置 | `_backup/{tree,sequence,circular,state}/` 共 ~63 个旧 DSL；夹具未定义毕业路径 |

## 3. 总体架构

```
showcase/{layout}/*.pgm              样例源（layout = 引擎注册名；文件名即角色）
        │                              一期仅 hierarchical/；tree/… 毕业后同级出现
        ▼  showcase/render.sh（薄壳编排）
        │     └─ scripts/*.py          发现样例 / 增量判定 / 写 manifest.json
showcase/_out/{layout}/…             SVG 产物（gitignore）
showcase/_out/manifest.json          画廊与门禁共同数据源（gitignore）
        │
        ├── index.html               进仓库的静态画廊（手写；运行时 fetch manifest）
        │
        ▼  plotgram measure（布局无关起步指标 + 可选 layout 扩展字段）
benchmarks/baselines/latest.json     门禁基线（进仓库；按 path 含 layout）
        │
        ▼  benchmarks/compare.sh（角色分轨棘轮；sets 路径带 layout 前缀）
CI gate（smoke + product 硬门禁；新 layout 自带 smoke 后再进 CI）
```

单一事实源原则：`.pgm` 文件是唯一进仓库的样例资产；SVG 与 `manifest.json` 全部由脚本派生、gitignore。`index.html` **进仓库且不由脚本生成**（只读 manifest）。基线只提交**指标 JSON**，不提交 SVG。

## 4. 目录与命名（多布局约定）

```
showcase/
├── hierarchical/            # 一期：{facet}/{role}.{slug}.pgm
│   ├── flat/ group/ partition/
├── tree/                    # 将来：tree 内核毕业后从 _backup 迁入 / 重写
├── sequence/                # 将来
├── circular/                # 将来
├── _backup/                 # 未支持内核的旧 DSL；永不进渲染 / 门禁
│   ├── tree/                # 11
│   ├── sequence/            # 22
│   ├── circular/            # 13
│   └── state/               # 17（v1 按图种归档；毕业时拆入 hier/circular，见 §4.2）
├── _out/                    # SVG + manifest.json（gitignore；镜像源路径）
├── scripts/                 # 编排辅助（进仓库；见 §6.1）
│   ├── discover.py          # 列出激活 layout 下的 .pgm（含 facet 子目录）
│   ├── write_manifest.py    # 写 / 比对 _out/manifest.json（含 facet / changed）
│   └── incremental.py       # mtime 增量判定
├── index.html               # 监视画廊（手写单文件；fetch _out/manifest.json）
├── render.sh                # 薄壳入口（调 CLI + scripts/）
└── assets/                  # 品牌资源
```

三层模型（layout → 可选 facet → role）以 [`showcase/README.md`](../../showcase/README.md) 为现行契约；本方案不重复细则。

### 4.1 不变式

- **目录名 = 布局内核注册名**（ADR-001），不是图种 / profile。禁止再建 `architecture/`、`mindmap/`、`flowchart/` 等图种目录。
- **facet 由各内核自定**，禁止把 hier 的 `flat/group/partition` 提升为全库强制目录。
- 角色前缀沿用：`smoke.` / `product.` / `demo.` / `stress.` / `mech.`（语义见现行 README）。
- 顶层除 `_` 前缀与已知非样例项（`index.html`、`render.sh`、`scripts/`、`assets/`）外，**每一个目录都是布局族**；`scripts/discover.py` **自动发现**，无第三份注册表。
- `_backup/` 与 `_out/` 永不被自动发现扫入。
- **删除**：`hierarchical/*.svg`（陈旧）；根目录 `update-gallery-manifest.py`（旧「改写 index.html 内联 SAMPLE_PATHS」路径废弃，逻辑迁入 `scripts/write_manifest.py` 写 JSON，不再碰 HTML）。

### 4.2 新布局毕业规程（tree / sequence / circular …）

内核达到「可对真实 DSL 出图」后，按同一清单挂入夹具（不另开方案）：

| 步骤 | 动作 |
|------|------|
| 1. 目录 | 建 `showcase/{layout}/`；从 `_backup/{layout}/` 挑选可迁移样例，**重写为现行 DSL**（不得原样搬 v1 语法） |
| 2. 角色最低集 | 至少：`smoke.`×1–2 + `product.`×若干；`demo.` / `stress.` / `mech.` 按需 |
| 3. 门禁集 | 在 `benchmarks/sets/` 增加或扩展清单，路径带 `{layout}/…`；新 layout 先只进本地 compare，**smoke 稳定后再入 CI gate** |
| 4. 基线 | `snapshot.sh` 采该 layout 子集，抬 `latest.json`（commit note 写 `raise {layout}/{role}: …`） |
| 5. 指标 | 共用 §5.2 起步集；若内核有专属正确性（如 sequence 生命线重叠），在 `measure` 增字段并注明 `schema_version`  bump |
| 6. 清理 | `_backup` 中已毕业且无保留价值的文件删除；无法映射现行模型的标注废弃 |

**`_backup/state/` 特例**：v1 按图种归档。毕业时按 profile→内核映射拆入目标目录（分层 state → `hierarchical/`，环形 → `circular/`），**不**建 `showcase/state/`。

### 4.3 一期范围

P0–P3 只保证 `hierarchical/` 闭环；画廊 / 脚本的 layout 过滤与自动发现**一并落地**（空目录不出现即可），避免 tree 落地时再改夹具。

## 5. CLI 前置能力（plotgram-cli 扩展）

现行薄壳 CLI 需要两个新子命令（参数解析在 CLI，逻辑在 crate）：

### 5.1 `plotgram validate <file>`

- 只做 parse + 模型校验，不布局不渲染；失败输出结构化诊断（对齐 `error-model.md`），exit ≠ 0。
- 用途：门禁第 0 轨、画廊状态标记。

### 5.2 `plotgram measure <file> [--json]`

输出单样例指标 JSON。指标集最小起步，分三轨：

| 轨 | 指标（起步集） | 性质 |
|----|----------------|------|
| 正确性 | `parse_error`；`node_overlap_count`；`edge_crosses_group_interior`；`label_overlap_count`；`det`（双跑字节一致，见 §8.3） | 硬，不可退化 |
| 质量 | `edge_crossing_count`；`total_edge_length`；`canvas_area`；`aspect_ratio` | 按角色分轨 |
| 观测 | `elapsed_ms`（release 计时）；`node_count` / `edge_count` | 只记录不门禁 |

实现位置：指标计算放 `plotgram-engine`（或新 `plotgram-audit` 模块，实现时定），只依赖 `LayoutContract` 结果几何，**不得按图种 / layout 名分支**（AGENTS.md §1/§3）。布局专属正确性指标以**可选字段**挂上（缺省即跳过），`schema_version` 供 compare 校验。

布局专属指标示例（内核落地时再加，非一期）：

| 内核 | 可能增补 | 轨 |
|------|----------|----|
| tree | `non_tree_edge_count`（输入约束违规观测） | 观测或正确性（待 tree 设计定） |
| sequence | 生命线重叠 / 消息时序倒挂计数 | 正确性 |
| circular | 分量未落环计数 | 正确性 |

## 6. 渲染脚本 `showcase/render.sh`（重写）

职责：**薄壳编排**——构建 CLI、驱动 validate/render、调用 `scripts/` 写 manifest、可选 serve。非平凡逻辑不堆在 bash 里。

```
./showcase/render.sh                  # 增量渲染全部激活布局目录 → _out/
./showcase/render.sh --layout tree    # 只渲一个布局族（开发 tree 时高频）
./showcase/render.sh --force          # 全量重渲
./showcase/render.sh --serve [PORT]   # 渲染后起静态服务（默认 4173）
./showcase/render.sh --debug          # 用 debug 二进制（默认 release）
```

### 6.1 `scripts/` 与 bash 分工

| 层 | 文件 | 做什么 |
|----|------|--------|
| 入口 | `render.sh` | `cargo build`；解析 flag；循环调 `plotgram validate` / 渲染；`--serve` 时 `python3 -m http.server` |
| 发现 | `scripts/discover.py` | 列出激活 layout 下 `.pgm`（尊重 `--layout`；跳过 `_backup` / `_out`） |
| 增量 | `scripts/incremental.py` | 比较源 / 二进制 / 产物 mtime 或内容 hash，输出「需要重渲」清单；`--force` 时全量 |
| 清单 | `scripts/write_manifest.py` | 渲染前备份 `manifest.json` → `manifest.prev.json`；渲染后写新 manifest（含 `layout` / `changed` / status） |

实现时可把 discover / incremental / write_manifest 收成一个 `scripts/gallery.py` 多子命令，目录仍叫 `scripts/`。依赖：**仅 Python 3 标准库**（与现行 `update-gallery-manifest.py` 同级），不引入 pip 包。

与旧路径的差异：

- **旧**：`update-gallery-manifest.py` 改写 `index.html` 里的内联 `SAMPLE_PATHS`。
- **新**：`index.html` 手写进仓库，运行时 `fetch('_out/manifest.json')`；Python **只写 JSON，不生成 / 不改写 HTML**。

### 6.2 规则

- 自动构建 `plotgram-cli`（默认 release，`--debug` 切换；遵守 AGENTS.md「验真」条，开发期可随时 `--debug`）。
- **发现范围**：`showcase/` 下非 `_` 前缀目录中的 `*.pgm`；一期自然只有 `hierarchical/`。
- **增量判定**：`.pgm` mtime/hash 或二进制 mtime 新于产物才重渲；`--force` 跳过判定。
- 每个样例先 `validate` 再渲染；两者状态都进 manifest。
- 输出 `_out/{path 去 .pgm}.svg`（镜像源树含 facet；透明背景）。

### 6.3 manifest.json（画廊与门禁的共同数据源）

```json
{
  "generated_at": "…",
  "binary_hash": "…",
  "samples": [
    {
      "path": "hierarchical/group/product.cloud-native.pgm",
      "layout": "hierarchical",
      "facet": "group",
      "role": "product",
      "status": "ok | parse-error | render-error",
      "svg": "_out/hierarchical/group/product.cloud-native.svg",
      "error": null,
      "elapsed_ms": 182,
      "hash": "…",
      "changed": true
    }
  ]
}
```

`layout` / `facet` / `role` 由路径派生（见 showcase README）。`changed`：与上一份 `_out/manifest.prev.json` 比对 hash（渲染前备份旧 manifest），让画廊一眼看出「这次迭代动了哪些图」。

## 7. 视觉监视画廊（index.html 重写）

保留单文件、零依赖的形态（现状优点）。**手写进仓库**，不由 `render.sh` / Python 生成；数据源改为运行时读取 `_out/manifest.json`（需经 `render.sh --serve` 或任意静态服务器打开，避免 `file://` 下 fetch 失败）。

### 7.1 核心监视能力（按优先级）

1. **状态徽章**：每卡片角标 ✅ / 解析错 / 渲染失败 / 🔄 有变化；顶部汇总条（总数 / 失败数 / 变化数；可按当前 layout 过滤子集汇总）。
2. **过滤**：layout × 角色 × 状态 三维过滤 + 文件名搜索。一期 layout 只有 hierarchical，控件仍要有（多 layout 时零改动）。高频视图：「只看有变化」「只看失败」「只看当前在改的 layout」。
3. **缩略图墙**：SVG 经 `IntersectionObserver` 懒加载；样例规模随 layout 毕业增长，首屏仍秒开。
4. **灯箱详情**：点卡片放大，支持缩放 / 1:1；并排展示 DSL 源（fetch `.pgm`）与渲染结果。
5. **键盘导航**：`j/k` 在过滤结果内切换，`Esc` 关闭灯箱——抽检 20 张图不用碰鼠标。

### 7.2 不做

- 不做实时 watch / WebSocket 热更（`render.sh` 足够快，重跑 + 刷新即可；未来需要再加）。
- 不做服务端：纯静态页 + `python3 -m http.server`。
- 不做按图种（flowchart / mindmap）过滤——画廊维度跟引擎一致，只认 layout。

## 8. 质量门禁

### 8.1 门禁集（benchmarks/sets/，路径更新为现行目录）

| 清单 | 角色 | 门禁行为 |
|------|------|----------|
| `product-regression-set.txt` | product | 正确性 + 质量**硬 FAIL** |
| `smoke-set.txt`（新增） | smoke | 正确性 + 质量硬 FAIL（小而快，CI 首选） |
| `stress-probe-set.txt` | stress | 正确性硬；质量 WARN（`--strict-stress` 改硬） |
| `demo-observe-set.txt` | demo | 正确性硬；质量 WARN |
| `mech-set.txt` | mech | 不进指标门禁；机制断言由 crate 内单元测试承担 |

清单内每行是相对 `showcase/` 的路径，**必须含 layout 前缀**（及 facet，若有；如 `hierarchical/flat/smoke.decision-loop.pgm`）。新 layout 毕业时往同一角色清单追加行，或另建 `{layout}-smoke-set.txt` 再在 CI 合并——优先前者，避免清单爆炸。

`_backup/` 下样例一律不进任何门禁集。新 layout 未达 smoke 稳定前，其路径**不写入** CI 所用的 smoke/product 清单。

### 8.2 快照与比对（重写 v1 脚本，接口保持习惯）

```bash
./benchmarks/snapshot.sh [--tag xxx] [--set path]   # plotgram measure 批量采集 → baselines/
./benchmarks/compare.sh baseline.json current.json  # 角色分轨棘轮，exit code 即门禁结论
```

- 快照产物沿用：`YYYY-MM-DD-HHMMSS[-tag].{json,md}` + `latest.{json,md}`。
- 比对规则沿用 v1 角色分轨表（正确性任何角色硬；质量 product/smoke 硬、demo/stress 软、mech 不门禁），指标字段换成 §5.2 起步集。
- **抬基线纪律**：基线 PR 必须在 commit note 写 `raise {layout}/{role}: 原因；残余: …`（沿用 v1 手册，加上 layout 分段）。

### 8.3 确定性校验（AGENTS.md 红线）

`measure` 对每个样例渲染两次，产物字节级比对，不一致则 `det=false` 且该样例直接 FAIL。不依赖 HashMap 序的要求在指标实现内同样适用。

### 8.4 CI 集成（.github/workflows/ci.yml 增加 job）

```
gate job:
  cargo build --release -p plotgram-cli
  对 smoke-set + product-regression-set 逐个 plotgram measure → current.json
  compare.sh baselines/latest.json current.json   # 硬门禁，非零即红
```

- 本地开发循环仍是：改布局 → `showcase/render.sh --serve` 肉眼看 → `snapshot.sh` + `compare.sh` 确认无退化 → 抬基线提交。
- v1 的 `bench-phases` 性能轨暂不移植；`elapsed_ms` 只进观测轨，性能门禁待引擎稳定后再立。

### 8.5 视觉棘轮（二期，可选项）

质量指标回退或 `changed` 样例需要人工判断「变好还是变坏」时，提供 diff 工具：`compare.sh --diff` 输出新旧 SVG 并排页（复用画廊灯箱组件）。一期先靠画廊「只看有变化」+ git 回退对比。

## 9. 实施分期

| 期 | 内容 | 完成判据 |
|----|------|----------|
| P0 清理 | 删陈旧 SVG；重写 sets 路径或标注废弃；README 同步；写明 `_backup` 毕业规则 | `grep` 无指向旧目录的活引用 |
| P1 视觉闭环 | CLI `validate`；`render.sh` + `scripts/`（发现 / 增量 / 写 manifest）；重写 `index.html`（fetch JSON + layout 过滤）；删根目录 `update-gallery-manifest.py` | hier 全量 69 样例 + 画廊可用；layout 维控件就位 |
| P2 指标门禁 | CLI `measure`；重写 `snapshot.sh` / `compare.sh`；采第一份 v2 基线（hier） | `compare.sh` 对同一快照自比零 diff；人为劣化能被拦 |
| P3 CI 门禁 | ci.yml gate job；smoke-set 建立（仅 hier） | PR 上劣化被 CI 挡下至少一次（演练） |
| P4（可选） | 视觉 diff / 基线趋势 viewer 移植 | — |
| L+ | 随 tree / sequence / circular 内核落地执行 §4.2 毕业规程 | 该 layout 的 smoke 进本地门禁；稳定后进 CI |

L+ 不阻塞 P0–P3；每个新内核自己的里程碑里带「showcase 毕业」一项即可。

## 10. 风险与取舍

- **指标起步集可能太粗**：宁可粗而硬（先挡住重叠/穿框/非确定），细化指标随布局开发逐条加入 `measure`，每次加入即纳入基线。避免 v1 后期「指标多而无人看」的教训。
- **SVG 不进仓库**：失去「打开 repo 就能看图」的便利，换来 diff 干净；画廊页 + CI artifact 可弥补（CI 可在 gate job 上传 `_out/` 为 artifact 供 PR 预览）。
- **`render.sh` 与 `snapshot.sh` 两份扫描逻辑**：样例发现复用 `scripts/discover.py`（或抽公共模块）；门禁仍以 sets 清单为准，不维护第三份注册表。
- **过早迁 `_backup`**：v1 DSL 与现行契约不兼容，强迁只会污染门禁；坚持「内核可跑 → 重写 → 毕业」。
- **基线混 layout**：`latest.json` 按 path 键控，新 layout 追加不影响 hier 棘轮；抬基线时按 layout 分段写 note。
- **bash 膨胀**：增量 / JSON 比对不进 shell；复杂逻辑进 `scripts/`，`render.sh` 保持可读编排。

## 11. 待确认问题

1. `measure` 指标起步集是否按 §5.2 表定稿，还是先只做正确性轨（质量轨 P3 再上）？
2. 陈旧 `hierarchical/*.svg` 直接删除确认（无留存价值，v1 渲染与新 DSL 不匹配）。
3. CI gate 是否需要上传 `_out/` 为 artifact 供 PR 视觉抽检。
4. 新 layout 进 CI 的门槛：仅要求 smoke 本地绿，还是必须先有独立 `product` 子集？
