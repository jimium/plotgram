// Drawify 编译管线与分散关注点（架构痛点示意）
// Mermaid 对照: 同目录 compiler-pipeline.mermaid
diagram architecture {
    title: "Drawify 编译管线与分散关注点"

    group pipeline "编译主流程" {
        entity parser_lexer "Parser/Lexer" { type: service }
        entity ast "AST" { type: service }
        entity validator "Validator" { type: service }
        entity layout_mod "Layout" { type: service }
        entity renderer_diagram "Renderer.diagram" { type: service }
        entity renderer_format "Renderer.format" { type: service }
    }

    group concerns "分散关注点" {
        entity diagram_semantics "图表类型语义" { type: external }
        entity default_layout "默认布局配置" { type: external }
        entity style_extension "样式扩展" { type: external }
        entity output_contract "输出契约" { type: external }
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
