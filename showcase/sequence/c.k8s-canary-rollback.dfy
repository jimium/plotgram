// Canary rollout rollback driven by health checks
// Mermaid mapping: complex sequence diagram for canary verification and rollback
diagram sequence {
    title: "K8s 金丝雀回滚链路"

    entity developer "开发者" { type: actor }
    entity ci "CI Pipeline" { type: control }
    entity argo "Argo Rollouts" { type: control }
    entity apiserver "API Server" { type: control }
    entity ingress "Ingress Gateway" { type: boundary }
    entity stable "Stable Pods" { type: boundary }
    entity canary "Canary Pods" { type: boundary }
    entity metrics "Prometheus" { type: database }
    entity alerts "告警系统" { type: boundary }

    developer -> ci "merge release branch"
    ci -> argo "publish canary spec"
    argo -> apiserver "create canary replica set"
    apiserver -> canary "start canary pods"
    argo -> ingress "route 10% traffic"
    ingress --> canary "sample live requests"
    canary -> metrics "report latency and errors"
    metrics --> argo "error rate rising"
    metrics --> alerts "threshold exceeded"
    alerts --> developer "rollback requested"
    argo -> ingress "route back to stable"
    ingress --> stable "100% traffic restored"
    argo -> apiserver "scale down canary pods"
    apiserver --> developer "rollback complete"
}
