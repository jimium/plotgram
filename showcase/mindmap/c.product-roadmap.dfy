// 产品路线图：多季度规划
// Mermaid 对照: 复杂 mindmap 多层级
diagram mindmap {
    title: "产品路线图 2025"

    entity roadmap "2025 路线图" { type: root }

    entity q1 "Q1 MVP" { type: main }
    entity q1_parser "解析器 v0.1" { type: leaf }
    entity q1_flowchart "流程图渲染" { type: leaf }
    entity q1_cli "CLI 工具" { type: leaf }

    entity q2 "Q2 扩展" { type: main }
    entity q2_sequence "时序图" { type: leaf }
    entity q2_arch "架构图" { type: leaf }
    entity q2_wasm "WASM 绑定" { type: leaf }

    entity q3 "Q3 生态" { type: main }
    entity q3_server "Web API" { type: leaf }
    entity q3_editor "在线编辑器" { type: leaf }
    entity q3_sdk "Agent SDK" { type: leaf }

    entity q4 "Q4 成熟" { type: main }
    entity q4_state "状态图" { type: leaf }
    entity q4_er "ER 图" { type: leaf }
    entity q4_perf "性能优化" { type: leaf }

    roadmap -> q1
    q1 -> q1_parser
    q1 -> q1_flowchart
    q1 -> q1_cli
    roadmap -> q2
    q2 -> q2_sequence
    q2 -> q2_arch
    q2 -> q2_wasm
    roadmap -> q3
    q3 -> q3_server
    q3 -> q3_editor
    q3 -> q3_sdk
    roadmap -> q4
    q4 -> q4_state
    q4 -> q4_er
    q4 -> q4_perf
}
