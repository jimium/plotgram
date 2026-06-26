# 矩形节点文字宽度自适应 — 可行性研究

> 研究日期：2026-06-24
> 研究对象：矩形节点根据文字标签宽度自动调整节点宽度
> 状态：可行性分析阶段

---

## 一、现状分析

### 1.1 已有基础机制

当前代码库中**已经实现了基于文字宽度的节点尺寸计算**，核心逻辑在
[node_sizing.rs](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/node/common/node_sizing.rs#L45-L51)：

```rust
pub fn standard_node_size(entity: &Entity) -> (f64, f64) {
    let width = (unicode_width::UnicodeWidthStr::width(entity.label.as_str()) as f64
        * LABEL_CHAR_WIDTH
        + LABEL_WIDTH_OFFSET)
        .clamp(MIN_NODE_WIDTH, MAX_NODE_WIDTH);
    layout::styled_node_size(entity, width, DEFAULT_NODE_HEIGHT)
}
```

### 1.2 当前参数配置

| 参数 | 当前值 | 说明 | 定义位置 |
|------|--------|------|----------|
| `LABEL_CHAR_WIDTH` | 11.0px | 每个"显示宽度单位"的像素值 | [node_sizing.rs#L16](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/node/common/node_sizing.rs#L16-L16) |
| `LABEL_WIDTH_OFFSET` | 44.0px | 左右 padding + border 总宽度 | [node_sizing.rs#L18](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/node/common/node_sizing.rs#L18-L18) |
| `MIN_NODE_WIDTH` | 96.0px | 节点最小宽度 | [node_sizing.rs#L20](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/node/common/node_sizing.rs#L20-L20) |
| `MAX_NODE_WIDTH` | 240.0px | 节点最大宽度 | [node_sizing.rs#L22](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/node/common/node_sizing.rs#L22-L22) |
| `DEFAULT_NODE_HEIGHT` | 40.0px | 默认节点高度 | [constants.rs](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/constants.rs) |

### 1.3 图标宽度叠加机制

节点图标（semantic icon）的宽度通过 [apply_icon_to_node_size](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/icons/layout.rs#L10-L29) 函数在基础宽度上叠加：

```rust
pub fn apply_icon_to_node_size(entity: &Entity, width: f64, height: f64, options: &ResolveOptions) -> (f64, f64) {
    // ...
    let extra = extra_node_width(def, DEFAULT_LABEL_FONT_SIZE); // icon_size + gap
    let width = (width + extra).max(def.min_node_width);
    // ...
}
```

该函数在 [styled_node_size](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/mod.rs#L1633-L1633) 末尾被调用，确保图标节点有足够空间。

### 1.4 使用该机制的布局算法

- `architecture_v2` — 架构图布局
- `flowchart` — 流程图布局
- `sugiyama_v2` — Sugiyama 层次布局
- `force_directed` — 力导向布局

---

## 二、当前存在的问题

### 2.1 字符宽度估算过于粗糙 ⚠️

**问题描述**：使用 `unicode_width::UnicodeWidthStr::width()` 统一乘以 11.0px，没有区分 ASCII 和 CJK 字符的实际宽度差异。

**边标签的对比参照**：边标签宽度估算 [estimate_label_width](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/edge/common/label_avoidance.rs#L393-L403) 已经做了 ASCII/CJK 区分：

```rust
pub fn estimate_label_width(text: &str) -> f64 {
    let mut width = 0.0;
    for ch in text.chars() {
        width += if ch.is_ascii() {
            DEFAULT_ASCII_CHAR_WIDTH  // 6.5px
        } else {
            DEFAULT_CJK_CHAR_WIDTH    // 11.0px
        };
    }
    width
}
```

**偏差对比（以 13px 字体为例）**：

| 文本内容 | unicode_width 估算 | ASCII/CJK 区分估算 | 偏差程度 |
|----------|-------------------|-------------------|----------|
| "hello"（5个英文字母） | 5 × 11 = 55px | 5 × 6.5 = 32.5px | **高估 69%** |
| "你好"（2个中文字） | 4 × 11 = 44px | 2 × 11 = 22px | **高估 100%** |
| "hello你好"（混合） | 9 × 11 = 99px | 5×6.5 + 2×11 = 54.5px | **高估 82%** |

> 注：`unicode_width` 对中文每个字返回宽度 2，所以"你好"的 width() 返回 4。

**影响**：
- 英文节点左右留白过多，视觉上不够紧凑
- 中英文节点宽度比例不协调
- 节点尺寸估算和边标签估算使用两套不一致的逻辑，增加维护成本

### 2.2 最大宽度限制可能导致文字溢出 📐

- `MAX_NODE_WIDTH = 240px`，超过此宽度后节点不再变宽
- SVG `<text>` 元素默认不裁剪溢出内容
- 长标签文字可能直接绘制到节点边界外部
- 没有省略号（ellipsis）或自动换行机制

### 2.3 字体大小未动态适配 🔠

- `LABEL_CHAR_WIDTH = 11.0` 是硬编码的固定值
- 节点字体大小可通过主题 / style 属性变化
- 字体变大时，文字宽度估算会偏小，可能导致文字溢出
- 字体变小时，节点显得过宽

### 2.4 不支持多行文字 📝

- 所有节点标签都是单行渲染（见 [render_centered_label](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/icons/render.rs#L130-L147)）
- 长文本没有自动换行机制
- 节点高度固定为 `DEFAULT_NODE_HEIGHT`
- 用户无法通过 DSL 控制换行

### 2.5 图标宽度计算可能未在所有路径生效

虽然 `styled_node_size` 调用了 `apply_icon_to_node_size`，但需要确认所有布局算法都通过 `styled_node_size` 来获取节点尺寸，而不是自行计算。

---

## 三、技术可行性评估

| 评估项 | 结论 | 说明 |
|--------|------|------|
| **技术可行性** | ✅ 完全可行 | 基础机制已存在，主要是精度优化和功能增强 |
| **改动范围** | 中等 | 核心在 `node_sizing.rs`，可能影响多个布局算法 |
| **风险等级** | 中低 | 主要风险是布局视觉变化，需要充分的回归测试 |
| **开发周期** | 1-5 天 | 取决于改进深度（见分阶段方案） |

### 3.1 为什么可行

1. **基础框架已就位**：`standard_node_size` + `styled_node_size` + `apply_icon_to_node_size` 的三层结构已经支持"根据内容计算尺寸"的模式
2. **有可复用的参考实现**：边标签的 `estimate_label_width` 已经实现了 ASCII/CJK 区分，可以复用思路
3. **无向后兼容约束**：根据 [AGENTS.md](file:///Users/jimichan/zaprt-projects/flowml/AGENTS.md)，项目尚未发布，可以自由调整
4. **所有布局算法统一入口**：大多数算法通过 `standard_node_sizes` 批量获取尺寸，修改一处即可全局生效

---

## 四、分阶段改进方案

### 阶段 1：修正宽度估算精度（优先级：高，预计 1-2 天）

**目标**：统一节点和边标签的宽度估算逻辑，使节点尺寸更准确、更紧凑。

#### 4.1.1 核心改动

**文件**：[node_sizing.rs](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/node/common/node_sizing.rs)

1. 新增 ASCII/CJK 区分的字符宽度计算函数，对齐 `estimate_label_width` 的逻辑
2. 重新校准 `LABEL_WIDTH_OFFSET`（左右 padding 值）
3. 可能需要调整 `MIN_NODE_WIDTH` 和 `MAX_NODE_WIDTH` 的值

#### 4.1.2 设计要点

- 复用 `constants::DEFAULT_ASCII_CHAR_WIDTH` 和 `constants::DEFAULT_CJK_CHAR_WIDTH`
- 保持 `unicode_width` 作为 fallback 或用于处理更复杂的 Unicode 字符
- 确保标点符号、数字、空格的宽度处理合理

#### 4.1.3 预期效果

- 英文节点宽度更紧凑，减少不必要的空白
- 中英文节点宽度比例更协调
- 节点尺寸估算与边标签宽度估算保持一致的口径
- 整体图表可能更紧凑，空间利用率更高

#### 4.1.4 测试要点

- 纯英文标签节点的宽度
- 纯中文标签节点的宽度
- 中英文混合标签的宽度
- 带图标节点的宽度（确保图标 + 文字都能放下）
- 极端长文本的截断效果
- 各布局算法的回归测试（architecture_v2、flowchart、sugiyama_v2、force_directed）

---

### 阶段 2：字体大小自适应 + 图标宽度校准（优先级：中，预计 1-2 天）

**目标**：考虑字体大小对文字宽度的影响，确保所有路径下图标宽度都被正确计算。

#### 4.2.1 字体大小自适应

**改动点**：
- 从实体样式 / 主题中读取字体大小（参考 [entity_label_font_size](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/render/color_queries.rs)）
- 基础字符宽度按字体大小比例缩放
- 默认字体大小下的行为与阶段 1 一致

**设计权衡**：
- 布局阶段需要访问主题/样式信息，可能需要调整参数传递方式
- 如果布局阶段拿不到字体大小，可以用默认值估算，接受小误差

#### 4.2.2 图标宽度校准

**改动点**：
- 审计所有布局算法的节点尺寸获取路径
- 确保都经过 `styled_node_size` → `apply_icon_to_node_size` 链路
- 补充缺失的图标宽度计算

---

### 阶段 3：多行文字 + 自动高度调整（优先级：低，预计 3-5 天，可选）

**目标**：长文本自动换行，节点高度随行数动态调整。

#### 4.3.1 核心功能

1. **文字换行算法**：
   - 输入：文本、最大宽度、字体大小
   - 输出：拆分成多行的文本数组
   - 策略：英文按单词边界断行，中文按字符断行
   - 最大行数限制，超出显示省略号

2. **节点高度计算**：
   - `高度 = 行数 × 行高 + 上下 padding`
   - 最小高度保证（单行时与现状一致）

3. **渲染层支持**：
   - 修改 [render_centered_label](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/icons/render.rs#L130-L147) 支持多行
   - 使用多个 `<tspan>` 元素或多个 `<text>` 元素
   - 行高、垂直居中逻辑调整

4. **DSL 控制（可选）**：
   - `node_sizing: auto` — 自动宽高（默认）
   - `node_sizing: fixed` — 固定宽高（当前行为）
   - `max_lines: 3` — 最大行数

#### 4.3.2 设计权衡

| 优点 | 缺点 |
|------|------|
| 长文本可读性更好 | 节点高度不一可能影响布局美观度 |
| 更灵活的内容展示 | 算法复杂度增加 |
| 符合用户直觉（文字多节点大） | 布局算法需要处理高度不一致的节点 |
| | 对某些布局算法（如层次布局）的对齐逻辑有影响 |

---

## 五、潜在风险与注意事项

### 5.1 布局稳定性风险 ⚠️

**风险**：节点宽度变化会影响所有依赖节点尺寸的布局算法，可能导致：
- 节点位置变化
- 边线路由变化
- 整体画布尺寸变化
- 某些场景下布局质量下降

**应对措施**：
- 准备一组典型的图表演示用例作为回归基准
- 分算法逐一验证
- 可以先作为实验性功能开关，逐步替换

### 5.2 字体度量精度限制

**问题**：纯 Rust 环境下无法精确测量文字宽度（依赖实际字体渲染引擎）。

**现状**：
- 我们使用经验值（ASCII 6.5px，CJK 11px）作为估算
- 实际宽度受字体家族、字重、hinting 等多种因素影响
- 估算值与实际渲染值始终有误差

**应对措施**：
- 保留合理的 padding 作为安全余量
- 以"宁宽勿窄"为原则，避免文字溢出
- 如果未来有 WASM 环境下的字体测量能力，可以进一步精确化

### 5.3 性能影响

- 字符级遍历计算宽度的复杂度是 O(n)，n 为标签长度
- 对大多数场景（标签长度 < 100 字符）可忽略不计
- 如果引入复杂的换行算法，考虑缓存计算结果

### 5.4 与显式样式的优先级

用户通过 `style: { width: 200 }` 显式设置的宽度应**优先**于自动计算。

这一点当前 [styled_node_size](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/mod.rs#L1611-L1634) 已经正确处理了：
- 先读取 style 中的 width/height，有则使用
- 没有则使用传入的默认值（自动计算的值）

---

## 六、相关文件索引

| 文件 | 作用 |
|------|------|
| [node_sizing.rs](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/node/common/node_sizing.rs) | 节点尺寸计算核心逻辑 |
| [layout.rs (icons)](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/icons/layout.rs) | 图标尺寸叠加逻辑 |
| [render.rs (icons)](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/icons/render.rs) | 节点内容渲染（文字+图标） |
| [label_avoidance.rs](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/edge/common/label_avoidance.rs) | 边标签宽度估算（参考实现） |
| [constants.rs](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/constants.rs) | 布局常量定义 |
| [mod.rs (layout)](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/layout/mod.rs) | `styled_node_size` 函数 |
| [node.rs (paint)](file:///Users/jimichan/zaprt-projects/flowml/crates/drawify-core/src/render/paint/node.rs) | 节点 SVG 绘制入口 |

---

## 七、总结与建议

### 7.1 核心结论

**矩形节点根据文字宽度自动调整框宽 — 完全可行。**

基础机制已经存在，当前主要问题是估算精度不够和功能完整性不足。通过分阶段优化，可以逐步提升效果。

### 7.2 建议实施路径

1. **优先实施阶段 1**（修正宽度估算精度）
   - 投入产出比最高
   - 改动量小，风险可控
   - 能明显改善英文节点的视觉效果

2. **根据效果决定是否推进阶段 2**
   - 如果字体大小变化的场景不多，可以延后
   - 图标宽度校准可以作为独立的小任务穿插进行

3. **阶段 3 按需评估**
   - 多行文字是较大的功能增强
   - 需要评估实际需求和投入产出比
   - 可以先收集用户反馈再决定

### 7.3 下一步行动建议

- [ ] 确认是否开始实施阶段 1
- [ ] 准备回归测试用例集（典型图表演示）
- [ ] 评估对现有布局算法的视觉影响
