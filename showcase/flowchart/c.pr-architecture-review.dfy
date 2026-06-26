// PR architecture review workflow with AI-generated diagrams and reviewer gates
// Mermaid mapping: complex flowchart for review automation and human approval
diagram flowchart {
    title: "PR 架构评审流程"
    config {
        direction: top-to-bottom
    }

    entity[start] pr_open "PR 创建"
    entity[process] collect "收集变更上下文"
    entity[process] generate "生成架构图草稿"
    entity[process] validate "校验 Drawify 语法"
    entity[decision] valid_gate "语法通过？"
    entity[process] repair "自动修复图表"
    entity[process] render "渲染 SVG 与摘要"
    entity[decision] review_gate "需要人工评审？"
    entity[process] human_review "架构师评审"
    entity[decision] approval_gate "评审通过？"
    entity[process] comment "回写 PR 评论"
    entity[process] update "开发者更新实现"
    entity[end] done "评审完成"

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
