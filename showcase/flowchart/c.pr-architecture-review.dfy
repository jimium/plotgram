// PR architecture review workflow with AI-generated diagrams and reviewer gates
// Mermaid mapping: complex flowchart for review automation and human approval
diagram flowchart {
    title: "PR 架构评审流程"
    config {
        direction: top-to-bottom
    }

    entity pr_open "PR 创建" { type: start }
    entity collect "收集变更上下文" { type: process }
    entity generate "生成架构图草稿" { type: process }
    entity validate "校验 Drawify 语法" { type: process }
    entity valid_gate "语法通过？" { type: decision }
    entity repair "自动修复图表" { type: process }
    entity render "渲染 SVG 与摘要" { type: process }
    entity review_gate "需要人工评审？" { type: decision }
    entity human_review "架构师评审" { type: process }
    entity approval_gate "评审通过？" { type: decision }
    entity comment "回写 PR 评论" { type: process }
    entity update "开发者更新实现" { type: process }
    entity done "评审完成" { type: end }

    pr_open -> collect
    collect -> generate
    generate -> validate
    validate -> valid_gate
    valid_gate -> render "是"
    valid_gate -> repair "否"
    repair -> generate
    render -> review_gate
    review_gate -> human_review "是"
    review_gate -> comment "否"
    human_review -> approval_gate
    approval_gate -> comment "是"
    approval_gate -> update "否"
    update -> collect "重新分析"
    comment -> done
}
