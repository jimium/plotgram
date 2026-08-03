# plotgram-router

独立边路由 crate。与 `plotgram-engine`（布局 facade）物理隔离，只依赖底层 API 和算法库。

## 依赖关系

```text
plotgram-model          (几何原语: Point, Rect, Side)
     ^
plotgram-engine-api     (RouteScene, EdgeRouter trait, LayoutError)
     ^
plotgram-algo           (VPSC, crossing, path_ortho)
     ^
plotgram-router         ← 本 crate
     ^
plotgram-engine         (facade; 消费 router)
```

**纪律**：本 crate 永不依赖 `plotgram-engine` 或任何 layout 实现。

## 模块结构

```text
src/
  lib.rs            # crate 入口
  core/             # 无策略几何原语（port_anchor, orthogonal_elbow, rect 工具）
  orthogonal/       # OrthogonalEdgeRouter（ovg / search / track / nudge）
  straight.rs       # StraightEdgeRouter（端子直连）
  polyline.rs       # PolylineEdgeRouter（可见性图 + Dijkstra）
  octilinear.rs     # OctilinearEdgeRouter（H/V/45° + 线桶）
  curved.rs         # CurvedEdgeRouter（贝塞尔 / Chaikin）
  verify.rs         # 几何不变量验证（正交、附着、净空、确定性）
  score.rs          # 质量度量（弯折、长度、交叉、共线）
  fixture.rs        # 场景夹具文件格式（BoardFixture 离散网格 + SceneFixture legacy + Requires 能力标记）
```

## 算法列表

| 名称 | 注册名 | 状态 | 说明 |
|------|--------|------|------|
| OrthogonalEdgeRouter | `orthogonal` | **M0–M2 已落地** | OVG+A*；两轮 shared；L3 `interval_color` + L4 VPSC nudge；单层组+gate；嵌套 scope 未做 |
| StraightEdgeRouter | `straight` | **已落地** | 端子两点直连；不避障；有组场景诚实拒绝 |
| PolylineEdgeRouter | `polyline` | **MVP 已落地** | 角点可见性图 + 欧氏 Dijkstra；组场景诚实拒绝 |
| OctilinearEdgeRouter | `octilinear` | **MVP 已落地** | H/V/45°；interesting-line + 线桶；组场景诚实拒绝 |
| CurvedEdgeRouter | `curved` | **MVP 已落地** | 端口贝塞尔采样；穿障回退 polyline+Chaikin |

新算法在 `examples/router_bench.rs` 和 `examples/viz.rs` 的 `lookup_algorithm()` 中注册即可被所有脚本自动发现。

## 测试

```bash
# 门禁（CI 自动跑）
cargo test -p plotgram-router
```

断言内容：

- 所有场景：正交性 + 端点附着 + 确定性
- `requires: none` 场景：全项通过（含障碍净空）
- `requires: search` 场景：`fixture_clearance_m0` 全项通过（M0 已解锁）
- `requires: track` 场景：`fixture_track_separation` 全项通过 + 多边不完全重合（M1 已解锁）
- `requires: group` 场景：`fixture_group_crossing` 全项通过（含 `group_clearance`）；`expect: no_path|unsupported` 由 `fixture_group_expect_fail` 验收

场景文件：`tests/scenes/*.json`（文本真源，可直接编辑/diff）。采用 **BoardFixture** 离散网格格式：

```json
{
  "id": "L01",
  "name": "blocker_center",
  "level": 2,
  "requires": "search",
  "description": "...",
  "cell": 10.0,
  "obstacles": [
    {"id": "a", "r": 2, "c": 0, "w": 8, "h": 4},
    {"id": "blocker", "r": 0, "c": 15, "w": 8, "h": 8}
  ],
  "edges": [
    {"id": "e0", "from": ["a", "east"], "to": ["b", "west"]}
  ]
}
```

- **坐标**：`r` = 行（y，向下增长），`c` = 列（x，向右增长）；世界坐标 = `(c*cell, r*cell)`，`cell` 默认 10。
- **障碍物**：矩形 `(r, c, w, h)`，单位为格。
- **端口**：`[node_id, side]` 自动取边中点；可选第三元素 `slot`（整数格偏移，正 = 下/右），如 `["a", "east", -1]`，用于并行边错开端口。
- **params**：`null` = 默认；可填 `OrthogonalRouteParams` 覆盖。

> ⚠ 棋盘是**夹具记法**，不是路由搜索图。路由仍走 reduced interesting lines OVG + A*（见 `docs/design/routing/orthogonal/architecture.md` §5），不违反 AGENTS.md §2"均匀网格禁作产品路径"。

## 脚本工具

所有脚本在 `scripts/` 下，从 crate 根目录或 workspace 根目录均可执行。

### viz.sh — 可视化

```bash
./scripts/viz.sh [algorithm]    # 默认 orthogonal；内部 --release（debug 全量夹具会极慢）
```

生成自包含 HTML（SVG 绘制 node + edge path），自动打开浏览器。
输出：`target/router-viz.html`

### score.sh — 打分基线管理

```bash
./scripts/score.sh baseline [algo]   # 存当前分数为基线
./scripts/score.sh compare [algo]    # 对比当前 vs 基线（默认子命令）
./scripts/score.sh update [algo]     # 覆写基线
```

- 基线文件：`scripts/baseline.json`（进 git）
- 对比输出：终端表格 + stderr JSON（agent 可解析）
- Exit code：`0` = 无回归，`1` = 净空回归，`2` = 无基线文件

## 开发循环

```text
改算法 → cargo test -p plotgram-router（门禁绿？）
       → ./scripts/score.sh compare（分数好了？）
       → ./scripts/viz.sh（目视路径合理？）
       → ./scripts/score.sh update（确认改进，更新基线）
```

## 里程碑

与 [architecture.md](../docs/design/routing/orthogonal/architecture.md) §10 同步：

| 阶段 | 状态 | 目标 | 解锁 |
|------|------|------|------|
| M0 | **已落地** | OVG + A* 避障搜索 | search 场景全 PASS（`fixture_clearance_m0`） |
| M1 | **已落地** | 两轮 shared / 走廊 track 分离 / 规模门控 / min_segment | track 场景不重合（`fixture_track_separation`，全 20 场景 PASS） |
| M2 | **部分** | 组边界穿越（类型 + 诚实拒绝已落地；穿越模型 / L4 VPSC nudging 未做） | `requires: group` 场景 |
| M3 | **部分** | Hier `DeferToRouter` 投影集成已通；Tree 接入 / FacadeVerifier 未做 | layout 冻节点后 path 可换 |
