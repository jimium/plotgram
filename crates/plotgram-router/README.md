# plotgram-router

独立正交边路由 crate。与 `plotgram-engine`（布局 facade）物理隔离，只依赖底层 API 和算法库。

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
  orthogonal/       # OrthogonalEdgeRouter 实现
  verify.rs         # 几何不变量验证（正交、附着、净空、确定性）
  score.rs          # 质量度量（弯折、长度、交叉、共线）
  fixture.rs        # 场景夹具文件格式（SceneFixture + Requires 能力标记）
```

## 算法列表

| 名称 | 注册名 | 状态 | 说明 |
|------|--------|------|------|
| OrthogonalEdgeRouter | `orthogonal` | **stub** | 单弯折 L 形直连，无避障。M0 将替换为 OVG + A* |

新算法在 `examples/bench.rs` 和 `examples/viz.rs` 的 `lookup_algorithm()` 中注册即可被所有脚本自动发现。

## 测试

```bash
# 门禁（CI 自动跑）
cargo test -p plotgram-router
```

断言内容：

- 所有场景：正交性 + 端点附着 + 确定性
- `requires: none` 场景：全项通过（含障碍净空）
- `requires: search` 场景：`#[ignore]`，M0 实现后解锁

场景文件：`tests/scenes/*.json`（文本真源，可直接编辑/diff）。

## 脚本工具

所有脚本在 `scripts/` 下，从 crate 根目录或 workspace 根目录均可执行。

### viz.sh — 可视化

```bash
./scripts/viz.sh [algorithm]    # 默认 orthogonal
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

### gen-scenes（example）

```bash
cargo run -p plotgram-router --example gen-scenes
```

从程序化定义导出 `tests/scenes/*.json`。一次性生成器，日常直接编辑 JSON。

## 开发循环

```text
改算法 → cargo test -p plotgram-router（门禁绿？）
       → ./scripts/score.sh compare（分数好了？）
       → ./scripts/viz.sh（目视路径合理？）
       → ./scripts/score.sh update（确认改进，更新基线）
```

## 里程碑

| 阶段 | 目标 | 解锁 |
|------|------|------|
| 当前 | stub + 测试基础设施 | `requires: none` 场景全绿 |
| M0 | OVG + A* 避障搜索 | `fixture_clearance_m0` 解锁，全 8 场景 PASS |
| M1 | 多边分离（track/nudging） | parallel 场景不重合 |
| M2 | 组边界穿越 | `requires: group` 场景 |
