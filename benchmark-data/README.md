# benchmark-data

布局 / 边路由的**回归门禁数据与工具**。

完整说明（含流程 / 指标图解）见可读版：

- 文件：[`README.html`](./README.html)
- 本地服务：`./benchmark-data/serve-viewer.py` → http://127.0.0.1:8765/readme

**产品规定（共线 / 合流验收语言）**：  
[`docs/architecture/方案计划/collinear-and-arrow-merge-comparison.md`](../docs/architecture/方案计划/collinear-and-arrow-merge-comparison.md) §2  
（Allowed / NeedsSeparation / Degraded；不以「共线计数归零」为成功标准。）

## 速查

```bash
# 采快照
./benchmark-data/snapshot-collinear.sh

# 对比门禁
./benchmark-data/compare-collinear.sh \
  benchmark-data/collinear-baseline-latest.json \
  path/to/new-snapshot.json

# 可视化基线变化 + 打开文档
./benchmark-data/serve-viewer.py
```

当前门禁指针：`collinear-baseline-latest.json`。

`compare-collinear.sh` 摘要：

| 轨 | 检查 |
|----|------|
| 正确性（硬） | `edge_crosses_group_interior` 不升；`det=true` |
| 质量（默认真 / 可债） | `exact_sev` / `tight_sev`；lint through/trunk/err；**`ortho.degraded_count` 不升**；perf；`node_fp` |
| 观测（WARN） | **`allowed_share_len` 可升**；若 allowed↑ 且 exact 未降 → 提示抽检误标 Allowed |

细节与图解见 HTML。
