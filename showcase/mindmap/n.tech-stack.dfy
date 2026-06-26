// 技术栈选型思维导图
// Mermaid 对照: mindmap 分类展开
diagram mindmap {
    title: "技术栈选型"

    entity stack "全栈技术" { type: root }

    entity frontend "前端" { type: main }
    entity react "React" { type: leaf }
    entity wasm "WASM" { type: leaf }

    entity backend "后端" { type: main }
    entity rust "Rust" { type: leaf }
    entity postgres "PostgreSQL" { type: leaf }

    entity devops "运维" { type: main }
    entity docker "Docker" { type: leaf }
    entity k8s "Kubernetes" { type: leaf }

    stack -> frontend
    frontend -> react
    frontend -> wasm
    stack -> backend
    backend -> rust
    backend -> postgres
    stack -> devops
    devops -> docker
    devops -> k8s
}
