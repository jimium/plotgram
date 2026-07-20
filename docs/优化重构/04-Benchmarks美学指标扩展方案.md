# Benchmarks 美学指标扩展方案

> 日期：2026-07-20
> 目的：补充当前门禁缺失的美学维度量化能力，使"路由是否变好看"可量化

---

## 1. 当前门禁的盲区

当前 `compare.sh` 检查的维度：
- ✅ 正确性：穿组（`edge_crosses_group_interior`）、确定性（`det`）
- ✅ 严重度：`exact_sev` / `tight_sev`（lint 违规加权）
- ✅ 共线/合流：Allowed / NeedsSeparation / Degraded
- ✅ 性能：10% budget
- ❌ **弯折数**：无
- ❌ **贴边距离**：无
- ❌ **对称性**：无
- ❌ **自环质量**：无
- ❌ **边交叉数**：无（lint 有 through 但不等于交叉）
- ❌ **路径长度分布**：无

**后果**：优化弯折/贴边/对称后，无法用门禁证明"变好了"或"没变差"。

---

## 2. 新增美学指标定义

### 2.1 弯折指标

```json
{
  "bends": {
    "total": 45,
    "avg_per_edge": 2.25,
    "max_single_edge": 7,
    "edges_with_excessive_bends": 2,
    "excessive_bend_threshold": 5
  }
}
```

**计算方式**：
- 每条边的弯折数 = `path_points.len() - 2`（去掉首尾端点）
- `excessive_bends`：弯折数 > 阈值的边数（阈值按图规模自适应：`max(4, node_count / 5)`）

### 2.2 组边框距离指标

```json
{
  "group_border_proximity": {
    "min_distance_px": 8.5,
    "edges_within_12px": 3,
    "edges_within_20px": 7,
    "hugging_violations": 1
  }
}
```

**计算方式**：
- 对每条边的每个段，计算到最近组边框的距离
- `hugging_violation`：距离 < 12px 且平行长度 > 40px 的段数
- 仅统计非走廊内的边（走廊内边天然靠近组边框，但应在走廊中心）

### 2.3 对称性指标

```json
{
  "symmetry": {
    "fan_out_nodes": 5,
    "avg_deviation": 0.08,
    "max_deviation": 0.23,
    "asymmetric_fan_outs": 1
  }
}
```

**计算方式**：
- 检测 fan-out 模式：节点 u 有 ≥2 个同层子节点
- 对称偏差 = |子节点重心 - 父节点中心| / 父节点宽度
- `asymmetric`：偏差 > 0.15 的 fan-out 数

### 2.4 自环质量指标

```json
{
  "self_loops": {
    "count": 3,
    "min_clearance_px": 12.0,
    "overlaps_with_neighbors": 0,
    "extends_outside_group": false
  }
}
```

**计算方式**：
- `clearance`：自环路径到最近非自身节点/组边框的距离
- `overlaps`：自环路径穿越邻居节点 bbox 的次数
- `extends_outside_group`：自环路径是否伸出所在组边框

### 2.5 边交叉指标

```json
{
  "crossings": {
    "total_edge_crossings": 4,
    "crossings_at_ports": 1,
    "crossings_in_channels": 2,
    "crossings_in_free_space": 1
  }
}
```

**计算方式**：
- 线段相交检测（排除共享端点的边对）
- 分类：端口附近（距端点 < 30px）/ 通道内 / 自由空间
- 端口附近交叉权重更高（视觉上更刺眼）

### 2.6 路径长度指标

```json
{
  "path_lengths": {
    "total_ink_px": 2450.0,
    "avg_edge_length_px": 122.5,
    "max_edge_length_px": 380.0,
    "length_cv": 0.45,
    "detour_ratio_avg": 1.35
  }
}
```

**计算方式**：
- `ink`：所有边路径总长度
- `detour_ratio`：路径长度 / 端点直线距离（1.0 = 完美直连）
- `cv`：变异系数（标准差/均值），衡量均匀性

---

## 3. 实现方案

### 3.1 数据采集位置

在 `plotgram-eval` crate 的评估逻辑中新增美学指标采集：

```rust
// crates/plotgram-eval/src/aesthetics.rs

pub fn compute_aesthetics(
    diagram: &Diagram,
    result: &LayoutResult,
) -> AestheticsReport {
    let bends = compute_bend_metrics(result);
    let border = compute_border_proximity(result);
    let symmetry = compute_symmetry(diagram, result);
    let self_loops = compute_self_loop_quality(diagram, result);
    let crossings = compute_crossing_metrics(result);
    let lengths = compute_path_length_metrics(result);
    AestheticsReport { bends, border, symmetry, self_loops, crossings, lengths }
}
```

### 3.2 门禁集成

在 `compare.sh` 中新增美学轨：

```python
# 美学轨（Phase A 期间为 WARN，稳定后转硬）
AESTHETICS_HARD = False  # Phase A 期间观测；Phase B 后转硬

def check_aesthetics(base_sample, cur_sample, role):
    issues = []
    b_aesth = base_sample.get("aesthetics", {})
    c_aesth = cur_sample.get("aesthetics", {})
    
    # 弯折数不升
    if c_aesth.get("bends", {}).get("avg_per_edge", 0) > \
       b_aesth.get("bends", {}).get("avg_per_edge", 0) + 0.2:
        issues.append("avg_bends_per_edge increased")
    
    # 贴边违规不升
    if c_aesth.get("group_border_proximity", {}).get("hugging_violations", 0) > \
       b_aesth.get("group_border_proximity", {}).get("hugging_violations", 0):
        issues.append("hugging_violations increased")
    
    # 对称偏差不升
    if c_aesth.get("symmetry", {}).get("avg_deviation", 0) > \
       b_aesth.get("symmetry", {}).get("avg_deviation", 0) + 0.05:
        issues.append("symmetry_deviation increased")
    
    return issues
```

### 3.3 角色分权

| 角色 | 美学轨行为 |
|------|-----------|
| product | Phase A: WARN → Phase B+: 硬 FAIL |
| stress | 始终 WARN（观测） |
| demo | 始终 WARN |
| mech | 不门禁 |

---

## 4. 新增 Product 样例

为覆盖美学维度，新增以下 product 门禁样例：

### 4.1 `product.self-loop-retry.pgm`

```
// 多自环场景：重试、状态回退、轮询
diagram flowchart {
    title: "重试与回退流程"
    config { direction: top-to-bottom }

    entity[start] init "初始化"
    entity[process] fetch "获取数据"
    entity[decision] check "数据有效？"
    entity[process] transform "转换处理"
    entity[decision] validate "校验通过？"
    entity[process] save "保存结果"
    entity[end] done "完成"

    init -> fetch
    fetch -> check
    check -> transform "有效"
    check -> fetch "无效，重试"
    transform -> validate
    validate -> save "通过"
    validate -> transform "失败，重做"
    save -> done
}
```

**覆盖**：2 个自环（check→fetch, validate→transform），测试自环方向选择和尺寸。

### 4.2 `product.symmetric-fanout.pgm`

```
// 对称 fan-out：决策分支应左右对称
diagram flowchart {
    title: "审批决策流程"
    config { direction: top-to-bottom }

    entity[start] submit "提交申请"
    entity[decision] review "主管审批"
    entity[process] approve "批准执行"
    entity[process] reject "驳回修改"
    entity[process] defer "暂缓处理"
    entity[end] notify "通知申请人"

    submit -> review
    review -> approve "同意"
    review -> reject "驳回"
    review -> defer "暂缓"
    approve -> notify
    reject -> notify
    defer -> notify
}
```

**覆盖**：3-way fan-out 对称性 + 3-way fan-in 汇聚。

### 4.3 `product.narrow-corridor.pgm`

```
// 窄组间隙：边不应贴组边框走
diagram architecture {
    title: "窄走廊穿越"

    group frontend "前端" {
        entity[frontend] web "Web"
        entity[frontend] mobile "Mobile"
    }

    group backend "后端" {
        entity[service] api "API"
        entity[service] worker "Worker"
    }

    group storage "存储" {
        entity[database] db "DB"
        entity[cache] cache "Cache"
    }

    web -> api
    mobile -> api
    api -> db
    api -> cache
    worker -> db
}
```

**覆盖**：三组垂直排列，组间隙可能较窄，测试边是否贴边。

---

## 5. 基线采集流程更新

```bash
# 采集含美学指标的新基线
./benchmarks/snapshot.sh --tag with-aesthetics

# 对比时自动检查美学轨
./benchmarks/compare.sh \
  benchmarks/baselines/latest.json \
  path/to/new-snapshot.json

# 显式忽略美学轨（Phase A 初期调试用）
./benchmarks/compare.sh --allow-quality-debt baseline.json current.json
```

---

## 6. 实施计划

| 步骤 | 内容 | 依赖 |
|------|------|------|
| 1 | 在 `plotgram-eval` 中实现美学指标采集 | 无 |
| 2 | 修改 `snapshot.sh` 输出美学字段 | 步骤 1 |
| 3 | 修改 `compare.sh` 增加美学轨检查 | 步骤 2 |
| 4 | 新增 3 个 product 样例 | 无 |
| 5 | 采集新基线（含美学） | 步骤 1-4 |
| 6 | Phase A 优化后对比验证 | 步骤 5 |
