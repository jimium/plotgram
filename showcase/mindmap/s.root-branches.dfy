// 学习计划：中心主题 + 三个分支
// Mermaid 对照: mindmap 基础三分支
diagram mindmap {
    title: "学习计划"

    entity plan "学习计划" { type: root }
    entity rust "Rust" { type: main }
    entity algo "算法" { type: main }
    entity eng "英语" { type: main }

    plan -> rust
    plan -> algo
    plan -> eng
}
