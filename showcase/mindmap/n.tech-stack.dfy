// 技术栈选型思维导图
// Mermaid 对照: mindmap 分类展开
diagram mindmap {
    title: "技术栈选型"

    entity[root] stack "全栈技术"

    entity[main] frontend "前端"
    entity[leaf] react "React"
    entity[leaf] wasm "WASM"

    entity[main] backend "后端"
    entity[leaf] rust "Rust"
    entity[leaf] postgres "PostgreSQL"

    entity[main] devops "运维"
    entity[leaf] docker "Docker"
    entity[leaf] k8s "Kubernetes"

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
