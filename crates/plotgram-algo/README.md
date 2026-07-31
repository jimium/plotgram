# plotgram-algo

共享**图绘制算法零件**（VPSC、FAS、交叉计数、方向变换、track 着色、正交路径规范化等）。

- **不是**布局引擎：无管线编排、无 `LayoutContract`、无 profile。  
- **消费者**：`plotgram-engine` 内的 `layout/*`、`route/*`（及将来独立抽出的实现 crate）。  
- **优先做哪些、验收什么**：见 [PARTS.md](PARTS.md)。

```text
plotgram-algo  ←  plotgram-engine (layout / route)
```

```bash
cargo test -p plotgram-algo
cargo check -p plotgram-algo
```
