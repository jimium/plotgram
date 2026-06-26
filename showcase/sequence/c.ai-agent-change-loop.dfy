// AI agent loop for architecture change analysis and diagram repair
// Mermaid mapping: complex sequence diagram for generate-validate-repair workflow
diagram sequence {
    title: "AI Agent 变更分析闭环"

    entity developer "开发者" { type: actor }
    entity repo "代码仓库" { type: database }
    entity agent "Diagram Agent" { type: control }
    entity context "Context Retriever" { type: control }
    entity validator "Drawify Validator" { type: control }
    entity patcher "Patch Agent" { type: control }
    entity renderer "Renderer" { type: control }
    entity review_bot "PR Review Bot" { type: boundary }

    developer -> repo "push change set"
    repo -> agent "emit repository event"
    agent -> context "load diff and related code"
    context --> agent "semantic context"
    agent -> validator "submit draft diagram"
    validator --> patcher "return structured errors"
    patcher --> agent "repair instructions"
    agent -> validator "submit revised diagram"
    validator -> renderer "validated diagram"
    renderer --> review_bot "svg and summary"
    review_bot --> developer "attach review comment"
}
