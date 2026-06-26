// AI-powered docops pipeline for generating, validating, and publishing diagrams
// Mermaid mapping: complex architecture graph for agent workflow and documentation automation
diagram architecture {
    title: "AI Agent 文档自动化管线"
    config {
        group_frame: stack {
            axis: horizontal
            track: equal
            cross: start
            gap: 50
            border: shared
        }
        edge_routing: orthogonal { bundling: 1.0 }
    }

    group inputs "输入源" {
        layout: horizontal

        entity[storage] repo "代码仓库"
        entity[service] pr "Pull Request"
        entity[service] cmdb "服务目录 / CMDB"
        entity[storage] prompt_lib "Prompt Library"
    }

    group agent_runtime "Agent 运行时" {
        layout: fan-out

        entity[service] orchestrator "Agent Orchestrator"
        entity[service] planner "Planning Agent"
        entity[service] graph_agent "Diagram Agent"
        entity[service] patch_agent "Patch Agent"
    }

    group drawify_stack "Drawify 栈" {
        layout: horizontal

        entity[service] validate "Drawify Validator"
        entity[service] render "Drawify Renderer"
        entity[service] diff "Diagram Diff Engine"
        entity[storage] artifact "Diagram Artifact Store"
    }

    group delivery "交付与反馈" {
        layout: horizontal

        entity[frontend] docs "Docs Portal"
        entity[service] review "PR Review Bot"
        entity[frontend] wiki "Knowledge Base"
        entity[service] telemetry "Agent Telemetry"
    }

    repo -> orchestrator "code context"
    pr -> orchestrator "change event"
    cmdb -> planner "service metadata"
    prompt_lib -> planner "prompt templates"
    orchestrator -> planner "analysis request"
    planner -> graph_agent "generate draft"
    graph_agent -> validate "diagram source"
    validate -> patch_agent "structured errors"
    patch_agent -> graph_agent "repair hints"
    validate -> render "validated diagram"
    render -> artifact "svg and json"
    artifact -> diff "baseline compare"
    diff -> review "change summary"
    artifact -> docs "publish diagram"
    artifact -> wiki "embed diagram"
    orchestrator -> telemetry "run metrics"
    review -> telemetry "review feedback"
}
