// 项目计划思维导图：两级展开
// Mermaid 对照: mindmap 多层级节点
diagram mindmap {
    title: "项目计划"

    entity project "Drawify 项目" { type: root }

    entity parser "解析器" { type: main }
    entity lexer "词法分析" { type: leaf }
    entity syntax "语法分析" { type: leaf }

    entity layout "布局引擎" { type: main }
    entity sugiyama "Sugiyama" { type: leaf }
    entity routing "边路由" { type: leaf }

    entity render "渲染器" { type: main }
    entity svg "SVG 输出" { type: leaf }
    entity ascii "ASCII 输出" { type: leaf }

    project -> parser
    parser -> lexer
    parser -> syntax
    project -> layout
    layout -> sugiyama
    layout -> routing
    project -> render
    render -> svg
    render -> ascii
}
