# 组穿越契约（M2-group 首期）

> 状态：**M2-group 首期已落地**（契约 + 夹具 + 碰撞/搜索/verify）
> 日期：2026-08-02
> 隶属：[architecture.md](../architecture.md) §5「组穿越」· [scope.md](../scope.md)
> 写权：[write-authority](../../../layout/write-authority.md) · R5 组场景诚实
> 代码：`fixture.rs` · `scene.rs::validate` · `orthogonal/ovg.rs` · `verify.rs` · `tests/scenes/group_*.json`

本文钉死独立正交 Router 首期组穿越的**最小语义**与 **BoardFixture 字段**。  
实现前先按本文升夹具、写红灯场景；Facade 投影组框可后置。

---

## 0. 一句话

```text
组框默认硬障碍
  + 边仅可按显式 BoundaryCrossing 经 gate_region 穿界
  + Router 不推断 LCA / 不发明门
  + 非法穿组 → 硬禁边（非大罚分）；无合法路径 → 硬失败
```

---

## 1. 首期范围（刻意收窄）

| 做 | 不做（后置） |
|----|--------------|
| 单层组（`group_boundaries` 互不嵌套、互不重叠） | 嵌套组 / scope 栈进 A* 状态 |
| 每条穿越 **必须** 带 `gate_region` | `gate_region: None`（整边任意穿） |
| 许可列表驱动的可走 gate 弧；违约硬过滤 | 违约高罚「尽量避免」 |
| 夹具手写 groups + crossings | Layout / Facade 投影组框（M3 续） |
| verify：非法穿组检测 | Gate 容量 / 多边挤门 / VPSC |

与 Hier Builtin Channel 的 GatePlan **语义对齐、策略不共享**：同一套「无许可不得穿」；搜索图仍是 OVG，不是 Channel substrate。

---

## 2. 输入语义

### 2.1 已有类型（`tautcore-engine-api::scene`）

```text
GroupBoundary     = { group_id, rect }
BoundaryCrossing  = { group_id, direction: Enter|Leave, gate_region: Option<Rect> }
boundary_permissions: EdgeId → BoundaryCrossing[]   # source→target 语义序
```

首期对 `gate_region` 的约束（比类型更严）：

| 规则 | 含义 |
|------|------|
| G1 | 出现在 `boundary_permissions` 里的每条 crossing：**`gate_region` 必填** |
| G2 | `gate_region` 必须与对应 `GroupBoundary.rect` 的**边界相交**（贴边或跨边一条轴对齐缝）；不得悬空在组内/组外 |
| G3 | `direction` 只描述语义（进/出）；几何上 gate 是双向可走弧，不另建方向边 |
| G4 | 同一 `(edge, group_id)` 允许成对 `Leave`+`Enter`（跨组）或单次（端点在组内）；**Router 不校验拓扑完备性以外的业务推断**——缺门导致无路则硬失败 |

### 2.2 障碍分层

| 几何 | 角色 |
|------|------|
| `obstacles[]`（节点等） | 与 M0/M1 相同：inflate(`spacing`) 硬障；端点自身节点免检 |
| `group_boundaries[]` | **默认硬障**（同样 inflate）；仅当该边持有对准该 `group_id` 的 crossing 时，**仅 gate 走廊**可穿过组框边界 |

组框**内部**对无许可边：不得进入（与穿边界同等硬禁）。  
有许可边：可经 gate 进入/离开；组内行走仍不得穿非自身节点障。

### 2.3 许可完备性（validate）

`RouteScene::validate` 从「有组就拒」改为结构检查：

1. `edge_order` ↔ `terminals` 键集一致（已有）。
2. 每个 `boundary_permissions` 键 ∈ `terminals`。
3. 每条 crossing 的 `group_id` ∈ `group_boundaries`。
4. 每条 crossing 满足 G1–G2。
5. 首期：任意两个 `group_boundaries.rect` 经 inflate(0) **不相交、无包含**（嵌套 → `UnsupportedRouteScene`，诚实拒绝）。
6. **有组但某边完全无 permissions**：合法——该边必须绕开所有组框（不得 silent 穿）。

不再因「仅存在 group_boundaries」而 `Unsupported`。

---

## 3. 搜索模型（L2 落点）

### 3.1 Interesting lines

在现有「障碍边 ± `(spacing+ε)` + 端子/stub」之上增加：

- 每个 `GroupBoundary.rect` 的四边 ± 同 offset；
- 每个 `gate_region` 的边线坐标（保证门缝落在格上）。

### 3.2 碰撞谓词（替换「有组就拒」）

对候选步 `a→b`（轴对齐）：

```text
若与某非免检 node obstacle 相交 → 禁
若与某 group G 的 inflate(rect) 相交：
  若该边 permissions 中不存在 G → 禁
  若存在 G，但 a→b 未落在任一对准 G 的 gate 走廊内 → 禁
  否则允许（经门穿界或门内短步）
```

**gate 走廊**：`gate_region` 再向外扩 `spacing` 量级的轴对齐通行带（实现标定；夹具用显式格对齐避免歧义）。首期允许简化为：段与 `padding(gate_region, spacing)` 相交且同时切入/切出组框，即视为经门。

### 3.3 A* 状态

首期 **不** 扩展为 `(vertex, Dir4, scope)`：

- 单层 + 硬过滤已足够表达合法性；
- 嵌套 / 必须按序进出多组时，再上提 scope 状态（届时改契约，不在夹具里偷加特判）。

代价仍为：长度 + `bend_penalty` × 弯（+ 可选 round-2 shared）。不设 `cluster_penalty` 进主搜（非法边已滤掉）。

### 3.4 失败语义

| 情况 | 结果 |
|------|------|
| validate 结构失败 | 硬失败 / `UnsupportedRouteScene`（嵌套等） |
| 预算内无碰撞合法路径 | 硬失败（该边）；禁止穿组单肘 fallback |
| Round-2 失败 | 与 M1 同：回退该边 round-1 |

---

## 4. 写权

| 自由度 | 写者 |
|--------|------|
| 组框几何 | Layout Metric（或夹具） |
| 每边 Crossing 列表与 gate | Layout Compose（或夹具） |
| 是否走某条 gate、组外绕行拓扑 | **本 Router L2** |
| 门内多边次序 / 偏移 | L3/L4（首期可共线；不单开组特判） |

Router **不得**：发明 gate、改 group rect、按「两端同组」自动补 permission。

---

## 5. BoardFixture 字段草案

在现有 `obstacles` / `edges` 旁增加可选组字段。棋盘仍是**记法**，不是搜索图。

### 5.1 形状

```json
{
  "id": "G01",
  "name": "group_bypass",
  "level": 3,
  "requires": "group",
  "description": "Group blocks the corridor; edge has no permission → must detour outside.",
  "cell": 10.0,
  "obstacles": [
    { "id": "a", "r": 4, "c": 0, "w": 8, "h": 4 },
    { "id": "b", "r": 4, "c": 30, "w": 8, "h": 4 }
  ],
  "groups": [
    { "id": "g0", "r": 2, "c": 12, "w": 10, "h": 8 }
  ],
  "edges": [
    {
      "id": "e0",
      "from": ["a", "east"],
      "to": ["b", "west"],
      "crossings": []
    }
  ],
  "params": null
}
```

经门穿越例：

```json
{
  "id": "G02",
  "name": "group_gate_enter",
  "level": 3,
  "requires": "group",
  "description": "Target inside group; explicit Enter via south gate.",
  "cell": 10.0,
  "obstacles": [
    { "id": "a", "r": 0, "c": 0, "w": 8, "h": 4 },
    { "id": "b", "r": 6, "c": 14, "w": 6, "h": 4 }
  ],
  "groups": [
    { "id": "g0", "r": 4, "c": 10, "w": 14, "h": 10 }
  ],
  "edges": [
    {
      "id": "e0",
      "from": ["a", "south"],
      "to": ["b", "north"],
      "crossings": [
        {
          "group": "g0",
          "dir": "enter",
          "gate": { "r": 4, "c": 14, "w": 6, "h": 1 }
        }
      ]
    }
  ]
}
```

### 5.2 字段表

| JSON | → RouteScene | 说明 |
|------|--------------|------|
| `groups[]` | `group_boundaries` | 同障碍：`id/r/c/w/h` → `group_id` + `Rect`；缺省 `[]` |
| `edges[].crossings` | `boundary_permissions[edge]` | 缺省 `[]`；有 `groups` 时可空（绕组） |
| `crossings[].group` | `group_id` | 必须 ∈ `groups[].id` |
| `crossings[].dir` | `direction` | `"enter"` \| `"leave"` |
| `crossings[].gate` | `gate_region: Some(Rect)` | **必填**（首期）；格单位矩形 |

转换规则：

- `groups` / `gate` 的世界坐标与 `obstacles` 相同：`(c*cell, r*cell, w*cell, h*cell)`。
- `requires: "group"`：集成测对该子集断言全量 verify（含未来的非法穿组检查）；实现前允许路由返回 `Unsupported` 的过渡测应删掉，改为红灯→绿灯。
- 无 `groups` 键或 `groups: []`：行为与今日无组场景 bit-identical。

### 5.3 场景表

| id | 意图 | 期望 |
|----|------|------|
| `group_bypass` | 组挡直线、`crossings: []` | 绕组外；path 不进组 inflate |
| `group_gate_cross` | 组挡路 + Leave/Enter 一门缝 | 路径经 gate；不从它处穿界 |
| `group_no_gate_fail` | 有组、无 crossings、绕行被封死 | 硬失败 `no collision-free path` |
| `group_nested_unsupported` | 两框包含 | `UnsupportedRouteScene` |
| `group_enter_inside` | 源外汇内 + Enter | 经北门进入后组内走到汇 |
| `group_leave_outside` | 源内汇外 + Leave | 经南门离开 |
| `group_two_gates` | 封闭腔 + 西 Enter / 东 Leave | 两门不同缝；不得外侧绕行（腔封死） |
| `group_shared_gate_two_edges` | 两边共北门进组内两目标 | 皆合法；路径不完全重合 |

---

## 6. 实现落点

| 步骤 | 落点 | 状态 |
|------|------|------|
| 1 | `fixture.rs`：解析 `groups` / `crossings` / `expect` | **已落地** |
| 2 | `scene.rs::validate`：§2.3；去掉「有组就拒」 | **已落地** |
| 3 | `ovg`：组线 + gate 线；`group_blocks_segment` / `step_blocked` | **已落地** |
| 4 | `verify`：`group_clearance` | **已落地** |
| 5 | `tests/scenes/group_*.json` + `fixtures.rs` | **已落地** |
| 6 | （后）`project_route_scene` 填组框与 permissions | 未做 |

---

## 7. 反模式

| 反模式 | 应做 |
|--------|------|
| 两端同组 → Router 自动补 permission | 夹具 / Compose 显式写出 |
| `gate_region: None` 当「整边可穿」 | 首期 validate 拒绝；后置另开里程碑 |
| 穿组单肘 fallback | 硬失败 |
| 用罚分代替禁边 | 非法步直接 `None` |
| 夹具用均匀网格当产品搜索图 | 棋盘仅记法；搜索仍 OVG |

---

> **摘要**：首期组穿越 = 单层硬障 + 必填 gate 的显式许可；夹具先表达 `groups`/`crossings`，再改 validate 与碰撞谓词；嵌套与 scope 状态后置。
