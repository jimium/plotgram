# 布局诊断（LayoutDiagnostics）使用指南

布局管线每次运行除了产出几何（节点 frame、边路径），还会带出一份**结构化观察报告**——`LayoutDiagnostics`：告诉你这次布局有没有被忽略的选项、参数指纹是什么、将来有没有发生过软放宽。

> 契约类型：`crates/tautcore-model/src/diagnostics.rs`  
> 设计依据：`docs/design/layout/hierarchical/architecture.md` §3.4、roadmap 阶段 C

---

## 它是干什么的

一句话：**布局的仪表，不是布局的旋钮。** Diagnostics 只报告事实，永远不影响几何结果。

解决三类问题：

| 问题 | 对应字段 |
|------|----------|
| 我写的 option 是不是被引擎忽略了？ | `warnings` |
| 这次渲染变化是参数变了还是代码变了？ | `params_hash` |
| 引擎有没有偷偷放宽我的约束？ | `relaxations`（当前无生产方，通道预留） |

---

## 与 Debug Trace 的区别

两者**并列产出，不合并**（debug-inspector.md §5.5）：

| | `LayoutDiagnostics` | `LayoutDebugTrace` |
|--|---------------------|--------------------|
| 回答什么 | 运行健康吗？参数是什么？ | 算法每一步是怎么决策的？ |
| 携带在哪 | `LayoutResult.diagnostics`（随产品输出） | 独立 JSON（`debug-layout` 子命令） |
| 消费方 | CLI / 回归测试 / 未来 verifier | inspector 叠层检视 |

---

## 类型结构

```rust
pub struct LayoutDiagnostics {
    pub warnings: Vec<LayoutWarning>,   // 非致命观察（如未知 option key）
    pub relaxations: Vec<Relaxation>,   // 软放宽记录（当前为空，通道预留）
    pub params_hash: String,            // 16 位 hex；绑定后参数的确定性指纹
}
```

- `params_hash` 是 FNV-1a 64 对 canonical 参数串的哈希：同一组绑定参数永远得到同一个值，任一参数变化都会换值。跨平台、跨工具链稳定（不用 `DefaultHasher`）。
- **硬失败不进诊断**：Unsupported / Infeasible 永远是 `Err`（如 `group_sizing` 这类当前无消费者的 option、VPSC 不可行），不会悄悄变成 warning。`edge_gap` 自 D1.0 起已有 TrackOrder 消费者，可正常 bind。

---

## 怎么用

### CLI：render（warnings 打到 stderr）

```bash
tautcore render my.taut -o out.svg
# stderr: warning: my.taut: unknown option `bogus_key`
```

warning 不影响退出码，stdout/输出文件仍是完整 SVG。

### CLI：measure（进 JSON 报告）

```bash
tautcore measure my.taut --json
```

```json
{
  "status": "ok",
  "diagnostics": {
    "warning_count": 1,
    "params_hash": "2a4aae85e0bfdde4"
  },
  ...
}
```

回归排查套路：同一 `.taut` 两次 measure，`params_hash` 不同 → 参数变了；相同但几何变了 → 代码变了。

### Rust API

```rust
use tautcore_compile::{build_layout, BuildOptions};

let result = build_layout(source, &BuildOptions::default())?;
for w in &result.diagnostics.warnings {
    eprintln!("layout warning: {}", w.message);
}
println!("params: {}", result.diagnostics.params_hash);
```

`LayoutResult` 可直接 serde 序列化，`diagnostics` 字段带 `#[serde(default)]`——旧 JSON 反序列化不受影响。

---

## 数据流

```text
HierarchicalParams::bind   →  warnings（未知 option key）
HierarchicalParams::hash() →  params_hash
        │
        ▼
compute() 组装 LayoutDiagnostics → LayoutOutput（engine-api）
        │
        ▼
run() / finalize() → LayoutResult.diagnostics（tautcore-model）
        │
        ▼
CLI render / measure · compile API · JSON 序列化
```

---

## 给维护者：什么时候往诊断里写东西

1. **warning 只放非致命观察**。能写成错误的（类型错、越界、无消费者的 option）必须是硬失败——宁可报错，不可静默。
2. **每次软放宽必须进 `relaxations`**（architecture.md §3.4）。D 阶段 Channel rip-up、Gate 回退等首批生产方落地时，`rule` 用固定词表命名（如 `"channel-rip-up"`），不得只留日志。
3. **诊断不得影响几何**。任何让几何随诊断内容变化的写法都是 bug——hier_eval 与坐标快照是门禁。
4. **不用墙钟超时改变布局结果**；搜索预算用确定性计数（候选数 / 轮数）。
