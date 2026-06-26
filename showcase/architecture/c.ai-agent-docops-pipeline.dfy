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

        entity repo "代码仓库" { type: storage }
        entity pr "Pull Request" { type: service }
        entity cmdb "服务目录 / CMDB" { type: service }
        entity prompt_lib "Prompt Library" { type: storage }
    }

    group agent_runtime "Agent 运行时" {
        layout: fan-out

        entity orchestrator "Agent Orchestrator" { type: service }
        entity planner "Planning Agent" { type: service }
        entity graph_agent "Diagram Agent" { type: service }
        entity patch_agent "Patch Agent" { type: service }
    }

    group drawify_stack "Drawify 栈" {
        layout: horizontal

        entity validate "Drawify Validator" { type: service }
        entity render "Drawify Renderer" { type: service }
        entity diff "Diagram Diff Engine" { type: service }
        entity artifact "Diagram Artifact Store" { type: storage }
    }

    group delivery "交付与反馈" {
        layout: horizontal

        entity docs "Docs Portal" { type: frontend }
        entity review "PR Review Bot" { type: service }
        entity wiki "Knowledge Base" { type: frontend }
        entity telemetry "Agent Telemetry" { type: service }
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
