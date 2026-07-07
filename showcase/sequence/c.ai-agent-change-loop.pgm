// AI agent loop for architecture change analysis and diagram repair
// Mermaid mapping: complex sequence diagram for generate-validate-repair workflow
diagram sequence {
    title: "AI Agent 变更分析闭环"

    entity[actor] developer "开发者"
    entity[database] repo "代码仓库"
    entity[control] agent "Diagram Agent"
    entity[control] context "Context Retriever"
    entity[control] validator "Drawify Validator"
    entity[control] patcher "Patch Agent"
    entity[control] renderer "Renderer"
    entity[boundary] review_bot "PR Review Bot"

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
