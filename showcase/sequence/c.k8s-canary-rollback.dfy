// Canary rollout rollback driven by health checks
// Mermaid mapping: complex sequence diagram for canary verification and rollback
diagram sequence {
    title: "K8s 金丝雀回滚链路"

    entity[actor] developer "开发者"
    entity[control] ci "CI Pipeline"
    entity[control] argo "Argo Rollouts"
    entity[control] apiserver "API Server"
    entity[boundary] ingress "Ingress Gateway"
    entity[boundary] stable "Stable Pods"
    entity[boundary] canary "Canary Pods"
    entity[database] metrics "Prometheus"
    entity[boundary] alerts "告警系统"

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
