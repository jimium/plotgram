// Drawify 编译管线与分散关注点（架构痛点示意）
// Mermaid 对照: 同目录 compiler-pipeline.mermaid
diagram architecture {
    title: "Drawify 编译管线与分散关注点"

    group pipeline "编译主流程" {
        entity[service] parser_lexer "Parser/Lexer"
        entity[service] ast "AST"
        entity[service] validator "Validator"
        entity[service] layout_mod "Layout"
        entity[service] renderer_diagram "Renderer.diagram"
        entity[service] renderer_format "Renderer.format"
    }

    group concerns "分散关注点" {
        entity[external] diagram_semantics "图表类型语义"
        entity[external] default_layout "默认布局配置"
        entity[external] style_extension "样式扩展"
        entity[external] output_contract "输出契约"
    }

    // 主流程
    parser_lexer -> ast
    ast -> validator
    ast -> layout_mod
    layout_mod -> renderer_diagram
    renderer_diagram -> renderer_format

    // 分散关注点（虚线 = 当前问题标注）
    diagram_semantics --> validator "当前分散"
    diagram_semantics --> renderer_diagram "当前分散"
    default_layout --> layout_mod "三处定义"
    default_layout --> renderer_diagram "三处定义"
    style_extension --> renderer_diagram "声明未落地"
    output_contract --> renderer_format "String 混合文本/二进制"
}
