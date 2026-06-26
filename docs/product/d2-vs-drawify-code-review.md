# D2 vs Drawify 深度源码对比与借鉴报告

> 基于 D2 (terrastruct/d2) 本地源码的深入分析，聚焦 Drawify 可借鉴的设计模式与实现方案。

## 目录

- [一、编译管线架构](#一编译管线架构)
- [二、插件系统](#二插件系统)
- [三、嵌套布局引擎](#三嵌套布局引擎)
- [四、Board 多层图表系统](#四board-多层图表系统)
- [五、自动格式化](#五自动格式化)
- [六、主题系统](#六主题系统)
- [七、LSP 支持](#七lsp-支持)
- [八、Exporter 与渲染分离](#八exporter-与渲染分离)
- [九、Sketch 渲染模式](#九sketch-渲染模式)
- [十、Drawify 已有优势](#十drawify-已有优势)
- [总结：借鉴路线图](#总结借鉴路线图)

---

## 一、编译管线架构

### D2：四阶段管线（AST → IR → Graph → Target）

D2 的编译管线是它最核心的架构设计：

```
d2parser.Parse() → d2ast.Map
    ↓
d2ir.Compile()   → d2ir.Map  （IR 中间表示）
    ↓
d2compiler.compileIR() → d2graph.Graph
    ↓
d2exporter.Export() → d2target.Diagram
```

**IR 层承担的语义工作**（`d2ir/compile.go`）：

1. **变量替换**（`compileSubstitutions`）：递归解析 `${vars.xxx}` 引用，维护 `varsStack` 逐层传递变量作用域
2. **Import 展开**（`d2ir/import.go`）：支持 `@path/to/file` 导入，带循环检测（`importStack`）
3. **Class 继承**（`overlayClasses`）：将根级 `classes` 定义覆盖到所有子 board
4. **Glob 模式匹配**（`globContext`）：`*` 和 `***` 通配符在 IR 阶段展开，支持 `&` 过滤器和 `!&` 排除
5. **Suspension 机制**：`suspend`/`unsuspend` 可以临时屏蔽/恢复字段

关键源码片段（`d2ir/compile.go`）：

```go
func Compile(ast *d2ast.Map, opts *CompileOptions) (*Map, []string, error) {
    c := &compiler{
        err: &d2parser.ParseError{},
        fs:  opts.FS,
        seenImports: make(map[string]struct{}),
    }
    m := &Map{}
    m.initRoot()
    c.compileMap(m, ast, ast)
    c.compileSubstitutions(m, nil)  // 变量替换
    c.overlayClasses(m)             // Class 继承
    m.removeSuspendedFields()       // 移除被 suspend 的字段
    return m, c.imports, nil
}
```

### Drawify：三阶段管线（Lexer → Parser → Expand）

```
lexer.tokenize() → Token流
    ↓
parser.parse()   → RawDiagram (AST)
    ↓
pipeline.prepare() → PreparedDiagram
```

Drawify 的 prepare 阶段目前只做两件事：补全 `entity.type` 默认值 + 物化样式到 `attributes.style`。

### 借鉴建议：引入 IR 层，解耦语义转换

**D2 的优势**：所有"语法糖"处理都集中在 IR 阶段，compiler 只需处理纯粹的语义映射。这让每个阶段的职责极其清晰。

**Drawify 的风险**：如果未来要支持变量、import、class 继承等特性，直接在 Parser 或 Expand 中堆砌会导致 `prepare()` 函数膨胀。

**具体建议**：

```rust
// 新增 IR 层
pub struct IrDiagram {
    pub diagram_type: DiagramType,
    pub entities: Vec<IrEntity>,
    pub relations: Vec<IrRelation>,
    // 变量作用域
    pub vars: HashMap<String, IrValue>,
}

// 管线变为：
// parse() → RawDiagram
// lower_to_ir() → IrDiagram    ← 新增
// prepare() → PreparedDiagram
```

**优先级：P2**（当需要变量/import 时再引入）

---

## 二、插件系统

### D2：Bundled + Binary 双轨插件

D2 的插件系统是它最精巧的设计之一（`d2plugin/`）：

**1. Plugin 接口**（`plugin.go`）：

```go
type Plugin interface {
    Info(context.Context) (*PluginInfo, error)
    Flags(context.Context) ([]PluginSpecificFlag, error)
    HydrateOpts([]byte) error
    Layout(context.Context, *d2graph.Graph) error
    PostProcess(context.Context, []byte) ([]byte, error)
}

type RoutingPlugin interface {
    RouteEdges(context.Context, *d2graph.Graph, []*d2graph.Edge) error
}
```

**2. Bundled Plugin**（`plugin_dagre.go`）：编译时内嵌，通过 `init()` 注册到全局 `plugins` 切片：

```go
func init() {
    plugins = append(plugins, &DagrePlugin)
}
```

**3. Binary Plugin**（`exec.go`）：通过 stdin/stdout JSON 协议通信：
- `d2plugin-xxx info` → 返回 `PluginInfo` JSON
- `d2plugin-xxx layout` → stdin 接收 `d2graph.Graph` JSON，stdout 返回布局后的 Graph
- `d2plugin-xxx postprocess` → stdin 接收 SVG bytes，stdout 返回处理后的 SVG

**4. Feature 声明**（`plugin_features.go`）：

```go
const NEAR_OBJECT PluginFeature = "near_object"
const CONTAINER_DIMENSIONS PluginFeature = "container_dimensions"
const TOP_LEFT PluginFeature = "top_left"
const DESCENDANT_EDGES PluginFeature = "descendant_edges"
const ROUTES_EDGES PluginFeature = "routes_edges"
```

`FeatureSupportCheck()` 在布局前检查 Graph 是否使用了插件不支持的功能，给出清晰的错误提示。

### Drawify：静态 Trait 注册

Drawify 的 `layout/mod.rs` 使用 Rust trait：

```rust
pub trait LayoutStrategy {
    fn name(&self) -> &'static str;
    fn compute(&self, diagram: &Diagram) -> LayoutResult;
    fn produces_edge_geometry(&self) -> bool { false }
    fn applicable_diagram_types(&self) -> &'static [DiagramType] { &[] }
    fn supports_custom(&self) -> bool { false }
}
```

所有算法通过 `all_layout_strategies()` 静态注册。

### 借鉴建议：Feature 声明 + 动态注册

**D2 的亮点**：`PluginFeature` + `FeatureSupportCheck` 是一个极好的设计——它让布局引擎声明自己支持什么，系统在运行时检查用户是否使用了不支持的特性。这比 Drawify 的 `applicable_diagram_types()` 更精细。

**具体建议**：

```rust
/// 布局引擎能力声明
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LayoutFeature {
    NearObject,           // 支持 near 指向另一个实体
    ContainerDimensions,  // 支持容器设置宽高
    LockedPosition,       // 支持 top/left 锁定位置
    DescendantEdges,      // 支持容器到后代的边
    EdgeRouting,          // 自行产出边几何信息
}

pub trait LayoutStrategy {
    // ... 现有方法
    fn features(&self) -> &[LayoutFeature] { &[] }
}

/// 在 compute_layout 之前检查
fn check_feature_support(
    strategy: &dyn LayoutStrategy,
    diagram: &PreparedDiagram,
) -> Result<(), Vec<DiagnosticError>> { ... }
```

**优先级：P1**（Feature 检查是低成本高收益的改进）

---

## 三、嵌套布局引擎

### D2：递归子图提取 + 注入

D2 的 `d2layouts/d2layouts.go` 实现了精巧的 `LayoutNested` 函数：

1. **自顶向下遍历**：BFS 遍历所有对象，检测嵌套的特殊图表类型（sequence、grid、constant-near）
2. **ExtractSubgraph**：将嵌套图表提取为独立子图，同时分离跨图边（external edges）
3. **递归布局**：对子图递归调用 `LayoutNested`
4. **InjectNested**：将布局结果注入回父图，`PositionNested` 做坐标偏移
5. **跨图边路由**：对跨图边单独调用 `edgeRouter`

关键源码片段：

```go
func LayoutNested(ctx context.Context, g *d2graph.Graph, graphInfo GraphInfo,
    coreLayout d2graph.LayoutGraph, edgeRouter d2graph.RouteEdges) error {

    // 1. BFS 遍历检测嵌套图表
    queue := make([]*d2graph.Object, 0, len(g.Root.ChildrenArray))
    queue = append(queue, g.Root.ChildrenArray...)

    for len(queue) > 0 {
        curr := queue[0]
        queue = queue[1:]
        gi := NestedGraphInfo(curr)
        if !gi.isDefault() {
            // 2. 提取子图
            nestedGraph, externalEdges, _ := ExtractSubgraph(curr, gi.IsConstantNear)
            // 3. 递归布局
            err := LayoutNested(ctx, nestedGraph, nestedInfo, coreLayout, edgeRouter)
            // ...
        }
    }

    // 4. 对父图执行主布局
    coreLayout(ctx, g)

    // 5. 注入子图 + 路由跨图边
    for _, id := range extractedOrder {
        InjectNested(obj, nestedGraph, true)
        PositionNested(obj, nestedGraph)
    }
    edgeRouter(ctx, g, extractedEdges)
}
```

### Drawify：单层布局

Drawify 目前对整个 Diagram 执行一次布局，没有嵌套子图的概念。

### 借鉴建议：支持嵌套子图布局

**具体建议**：在 `compute_layout` 中增加嵌套检测：

```rust
pub fn compute_layout(diagram: &PreparedDiagram) -> LayoutResult {
    // 1. 检测嵌套的特殊图表类型
    let nested = detect_nested_diagrams(diagram);

    // 2. 提取子图，递归布局
    for sub in nested {
        let sub_result = compute_layout(sub.diagram);
        // 3. 注入回父图
        inject_nested(sub.parent_id, sub_result, &mut result);
    }

    // 4. 对父图执行主布局
    // 5. 路由跨图边
}
```

**优先级：P2**（当支持 Board/多层图表时需要）

---

## 四、Board 多层图表系统

### D2：Layers / Scenarios / Steps

D2 的 `d2target/d2target.go` 定义了三种 Board：

```go
type Diagram struct {
    Layers    []*Diagram `json:"layers,omitempty"`
    Scenarios []*Diagram `json:"scenarios,omitempty"`
    Steps     []*Diagram `json:"steps,omitempty"`
}
```

- **Layers**：同一系统的不同抽象层级（如逻辑层/物理层）
- **Scenarios**：同一架构的不同场景（如正常/故障模式）
- **Steps**：时间序列步骤演示（如部署步骤）

Board 递归嵌套，通过 `GetBoard(path)` 路径查找。在 `d2compiler/compile.go` 中，`compileBoardsField` 递归编译每个 board：

```go
func (c *compiler) compileBoardsField(g *d2graph.Graph, ir *d2ir.Map, fieldName string) {
    for _, f := range boards.Map().Fields {
        g2 := d2graph.NewGraph()
        g2.Parent = g
        c.compileBoard(g2, m)  // 递归编译
        switch fieldName {
        case "layers":    g.Layers = append(g.Layers, g2)
        case "scenarios": g.Scenarios = append(g.Scenarios, g2)
        case "steps":     g.Steps = append(g.Steps, g2)
        }
    }
}
```

DSL 中的 Board 关键字定义在 `d2ast/keywords.go`：

```go
var BoardKeywords = map[string]struct{}{
    "layers":    {},
    "scenarios": {},
    "steps":     {},
}
```

### 借鉴建议：引入 Steps Board

**Steps** 对 Drawify 的 AI Agent 场景最有价值——Agent 可以生成一系列步骤图来演示流程演变。

**具体建议**：

```rust
// 在 Diagram AST 中增加
pub struct Diagram {
    pub diagram_type: DiagramType,
    // ... 现有字段
    pub steps: Vec<Diagram>,       // 步骤演示
    pub layers: Vec<Diagram>,      // 分层视图（后续）
}
```

DSL 语法：

```
diagram flowchart {
    steps: {
        "Step 1": { ... }
        "Step 2": { ... }
    }
}
```

**优先级：P1**（Steps 对 AI Agent 输出价值极高）

---

## 五、自动格式化

### D2：AST → String 反向序列化

D2 的 `d2format/format.go` 实现了完整的 AST → 源码格式化：

```go
func Format(n d2ast.Node) string {
    var p printer
    p.node(n)
    return p.sb.String()
}
```

关键设计：
- `printer` 维护缩进状态（`indentStr`），2 空格缩进
- 每个 AST 节点类型有对应的格式化方法
- Board 节点（layers/scenarios/steps）被提取到文件末尾
- 保留空行（`prev.GetRange().End.Line` 差值 > 1 时插入空行）
- BlockString 自动计算需要的 `|` 数量避免歧义
- 注释保持原位，内联注释检测同行性

关键源码片段：

```go
func (p *printer) _map(m *d2ast.Map) {
    // 提取 board 节点到末尾
    boardNodes := []d2ast.MapNodeBox{}
    for i, nb := range m.Nodes {
        if nb.IsBoardNode() {
            boardNodes = append(boardNodes, nb)
            prev = n
            continue
        }
        // 格式化普通节点...
    }
    // board 节点放到文件末尾
    for i, n := range boardNodes {
        if n.GetRange().Start.Line != 0 {
            p.sb.WriteByte('\n')
        }
        if i != 0 || len(m.Nodes) > len(boardNodes) {
            p.sb.WriteByte('\n')
        }
        p.sb.WriteString(p.indentStr)
        p.node(n)
    }
}
```

### Drawify：无格式化功能

### 借鉴建议：实现 DSL 格式化

**具体建议**：

1. 在 `PreparedDiagram` 上实现 `to_dsl()` 方法（反向序列化）
2. 实现独立的 `drawify fmt` 命令
3. 在 Playground 中集成 "Format" 按钮

```rust
pub fn format_dsl(diagram: &PreparedDiagram) -> String {
    let mut fmt = Formatter::new();
    fmt.format_diagram(diagram.inner());
    fmt.finish()
}
```

**优先级：P0**（AI 生成代码规范化是刚需）

---

## 六、主题系统

### D2：Token 体系 + 覆盖 + 特殊规则

D2 的 `d2themes/d2themes.go` 定义了 17 个颜色 token：

```go
type ColorPalette struct {
    Neutrals Neutral  // N1-N7: 最暗→最亮
    B1-B6              // 基础色：用于容器
    AA2, AA4, AA5      // 替代色 A
    AB4, AB5           // 替代色 B
}
```

**ThemeOverrides** 允许用户在 DSL 中覆盖特定 token：

```go
type ThemeOverrides struct {
    N1  *string `json:"n1"`
    B1  *string `json:"b1"`
    AA2 *string `json:"aa2"`
    // ... 17 个可选覆盖
}
```

**SpecialRules** 是 D2 主题的另一个精巧设计：

```go
type SpecialRules struct {
    Mono                       bool  // 等宽字体
    NoCornerRadius             bool  // 无圆角
    OuterContainerDoubleBorder bool  // 外层容器双线边框
    ContainerDots              bool  // 容器点阵填充
    CapsLock                   bool  // 全大写
    C4                         bool  // C4 模型风格
    AllPaper                   bool  // 全纸纹背景
}
```

这让同一个颜色 token 体系可以驱动完全不同的视觉风格（如 Origami 的无圆角 + 纸纹 vs C4 的虚线边框）。

`ApplyOverrides` 方法逐字段合并：

```go
func (t *Theme) ApplyOverrides(overrides *d2target.ThemeOverrides) {
    if overrides.B1 != nil { t.Colors.B1 = *overrides.B1 }
    if overrides.B2 != nil { t.Colors.B2 = *overrides.B2 }
    // ... 17 个字段
}
```

### Drawify：Theme + GraphicStyle 双轨

Drawify 的 `graphic_style/mod.rs` 分离了颜色（Theme）和笔触（GraphicStyle），这是 D2 没有的设计。Drawify 的 `StyleMap` 还携带了 `StyleSource` 溯源信息，这也是 D2 没有的。

### 借鉴建议：增加 ThemeOverrides + SpecialRules

**D2 的亮点**：`ThemeOverrides` 让用户无需创建完整主题就能微调颜色。`SpecialRules` 让主题能控制超越颜色的视觉属性。

**具体建议**：

```rust
pub struct StyleRequest {
    pub style_id: StyleId,
    pub graphic_style_id: GraphicStyleId,
    // 新增
    pub token_overrides: HashMap<String, String>,  // 如 {"canvas": "#101820"}
}

/// 主题特殊规则
pub struct ThemeRules {
    pub mono: bool,
    pub no_corner_radius: bool,
    pub outer_double_border: bool,
    pub container_dots: bool,
}
```

**优先级：P1**（token 覆盖是高频需求）

---

## 七、LSP 支持

### D2：基于 IR 的补全系统

D2 的 `d2lsp/completion.go` 实现了上下文感知的自动补全：

```go
func GetCompletionItems(text string, line, column int) ([]CompletionItem, error) {
    keyword := getKeywordContext(text, ast, line, column)
    switch keyword {
    case "style", "style.":
        return getStyleCompletions(), nil
    case "shape", "shape:":
        return getShapeCompletions(), nil
    // ... 20+ 种上下文
    }
}
```

它通过 `getKeywordContext` 判断光标所在的语义上下文（是在 style 块内？shape 值？near 值？），然后返回对应的补全列表。

### D2 Oracle：程序化编辑 API

`d2oracle/edit.go` 提供了 `Create`、`Set`、`Delete` 等 API，允许程序化修改图表而无需手动拼接 DSL 字符串：

```go
func Create(g *d2graph.Graph, boardPath []string, key string) (_ *d2graph.Graph, newKey string, err error) {
    newKey, edge, err := generateUniqueKey(boardG, key, nil, nil)
    if edge {
        err = _set(boardG, baseAST, key, nil, nil)
    } else {
        err = _set(boardG, baseAST, newKey, nil, nil)
    }
    g, err = recompile(g)
    return g, newKey, nil
}
```

这对 AI Agent 非常有用——Agent 可以通过结构化 API 操作图表，而非拼接文本。

### 借鉴建议：优先实现补全 + Oracle

**具体建议**：

1. **补全**：基于 Drawify 的 keywords 定义，实现 `get_keyword_context` + `get_completions`
2. **Oracle**：基于 Drawify 已有的 `diff/patch` 模块，实现 `create_entity`、`set_attribute`、`delete_entity` 等程序化编辑 API

Drawify 的 `DiagnosticError` 已经有结构化的 `Suggestion` + `FixAction`，这比 D2 的错误信息更适合 LSP 的 `codeAction`。

**优先级：P2**（补全）/ **P1**（Oracle，对 AI Agent 直接有用）

---

## 八、Exporter 与渲染分离

### D2：Graph → Target → Renderer 三层分离

D2 的渲染管线是：

```
d2graph.Graph → d2exporter.Export() → d2target.Diagram → d2svg.Render() → SVG bytes
```

`d2exporter` 将 Graph 转为纯数据结构 `d2target.Diagram`，包含 `Shape`、`Connection`、`Legend` 等。渲染器只消费这个纯数据结构。

关键源码片段（`d2exporter/export.go`）：

```go
func Export(ctx context.Context, g *d2graph.Graph, ...) (*d2target.Diagram, error) {
    diagram := d2target.NewDiagram()
    diagram.Shapes = make([]d2target.Shape, len(g.Objects))
    for i := range g.Objects {
        diagram.Shapes[i] = toShape(g.Objects[i], g)
    }
    diagram.Connections = make([]d2target.Connection, len(g.Edges))
    for i := range g.Edges {
        diagram.Connections[i] = toConnection(g.Edges[i], g.Theme)
    }
    return diagram, nil
}
```

### Drawify：PreparedDiagram → Renderer

Drawify 的渲染器直接消费 `PreparedDiagram`，没有独立的 Export 层。

### 借鉴建议：引入 Export 层

**D2 的优势**：Export 层将布局结果转为渲染器无关的纯数据结构，让多个渲染器（SVG、PNG、ASCII）共享同一份导出逻辑。

**建议**：Drawify 目前的 SVG 渲染器直接读取 `PreparedDiagram`，如果未来要支持 PNG/ASCII 等格式，应引入 Export 层将布局结果标准化。

**优先级：P3**（当需要多渲染器时再引入）

---

## 九、Sketch 渲染模式

### D2：基于 rough.js 的手绘效果

D2 的 `d2sketch/sketch.go` 通过嵌入 rough.js 实现手绘效果：

```go
//go:embed rough.js
var roughJS string

func Rect(r jsrunner.JSRunner, shape d2target.Shape, ...) (string, error) {
    js := fmt.Sprintf(`node = rc.rectangle(0, 0, %d, %d, {
        fill: "#000",
        stroke: "#000",
        strokeWidth: %d,
        %s
    });`, shape.Width, shape.Height, shape.StrokeWidth, baseRoughProps)
    paths, err := computeRoughPathData(r, js)
    // ...
}
```

它还定义了 `streaks.txt` 填充模式，给形状添加微妙的条纹效果，以及 `DefineFillPatterns` 函数根据亮度分类应用不同透明度的条纹。

### Drawify：Excalidraw GraphicStyle

Drawify 的 `excalidraw` 模块实现了类似的手绘效果，但通过 Rust 原生实现而非 JS 引擎。

**对比**：Drawify 的原生实现性能更好，但 D2 的 rough.js 生态更成熟（支持更多形状变体）。

---

## 十、Drawify 已有优势

以下是 Drawify 已经具备、但 D2 不具备的设计优势：

### 1. StyleSource 溯源

`StyleMap` 中每个属性都记录了来源（默认值/用户指定/主题继承），D2 没有这个机制。

```rust
pub struct StyleAttribute {
    pub value: AttributeValue,
    pub source: StyleSource,  // 溯源信息
}
```

### 2. 结构化诊断

`DiagnosticError` 有错误码、类别、修复建议（`FixAction`），D2 的错误只是字符串。

```rust
pub struct DiagnosticError {
    pub code: String,           // "E001", "E002"...
    pub severity: Severity,     // Error / Warning
    pub category: Category,     // Parse / Validation / Render
    pub message: String,
    pub location: Span,
    pub suggestion: Option<Suggestion>,  // 修复建议
}
```

### 3. AST Diff/Patch

Drawify 的 `diff/compare.rs` 支持语义级图表差异比较，D2 没有这个能力。

### 4. GraphicStyle 双轨

Theme（颜色）+ GraphicStyle（笔触）分离，比 D2 的 Theme + Sketch 开关更灵活。Drawify 支持 7 种笔触风格（Standard、Excalidraw、Blueprint、NeonGlow、SpatialClarity、Stipple、CrossHatch），D2 只有 Sketch on/off。

### 5. 降级解析

Parser 失败时返回部分 AST（`parse_file_fallback`）而非 nil，比 D2 更容错。

---

## 总结：借鉴路线图

| 优先级 | 借鉴项 | 来源模块 | 预期收益 | 实现难度 |
|--------|--------|----------|----------|----------|
| **P0** | DSL 自动格式化 (`drawify fmt`) | d2format | AI 生成代码规范化 | 中 |
| **P1** | LayoutFeature 声明 + 兼容性检查 | d2plugin/plugin_features | 减少运行时布局错误 | 低 |
| **P1** | Theme Token 局部覆盖 | d2themes.ThemeOverrides | 样式灵活性 | 中 |
| **P1** | Steps Board 支持 | d2target.Diagram.Steps | AI Agent 步骤演示 | 中 |
| **P1** | Oracle 程序化编辑 API | d2oracle | AI Agent 直接操作图表 | 中 |
| **P2** | IR 中间层 | d2ir | 架构可扩展性（变量/import/class） | 高 |
| **P2** | 嵌套子图布局 | d2layouts.LayoutNested | 复杂图表嵌套 | 高 |
| **P2** | LSP 自动补全 | d2lsp/completion | 开发体验 | 中 |
| **P3** | Export 层分离 | d2exporter | 多渲染器支持 | 中 |
| **P3** | Binary Plugin 协议 | d2plugin/exec | 第三方生态 | 高 |

### 核心结论

Drawify 在 AI Agent 友好性方面（结构化诊断、AST diff/patch、StyleSource 溯源、降级解析）已经领先 D2。最值得借鉴的是 D2 的**工程化成熟度**——自动格式化、Feature 声明检查、Board 系统、Oracle API 这些让工具从"能用"变成"好用"的关键基础设施。

### 本次分析新增发现（相比远程分析）

1. **IR 层的 glob/suspension 机制**：D2 的 IR 支持 `*`/`***` 通配符和 `suspend`/`unsuspend` 字段控制
2. **Plugin Feature 声明**：`FeatureSupportCheck` 在布局前校验用户是否使用了引擎不支持的功能
3. **嵌套子图的 Extract/Inject 模式**：`LayoutNested` 的递归提取-布局-注入-跨图边路由流程
4. **Oracle 程序化编辑 API**：`d2oracle` 的 `Create/Set/Delete` 让 AI Agent 可以程序化操作图表
5. **d2format 的 Board 节点后置**：格式化时将 layers/scenarios/steps 提取到文件末尾，保持主体内容在前

---

> D2 源码位于 `/tmp/d2-analysis`，可随时查阅。
