// 项目计划思维导图：两级展开
// Mermaid 对照: mindmap 多层级节点
diagram mindmap {
    title: "项目计划"

    entity[root] project "Drawify 项目"

    entity[main] parser "解析器"
    entity[leaf] lexer "词法分析"
    entity[leaf] syntax "语法分析"

    entity[main] layout "布局引擎"
    entity[leaf] sugiyama "Sugiyama"
    entity[leaf] routing "边路由"

    entity[main] render "渲染器"
    entity[leaf] svg "SVG 输出"
    entity[leaf] ascii "ASCII 输出"

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
