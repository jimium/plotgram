// Rolling update flow from CI to cluster traffic shift
// Mermaid mapping: complex sequence diagram for release automation
diagram sequence {
    title: "K8s 滚动发布链路"

    entity developer "开发者" { type: actor }
    entity ci "CI Pipeline" { type: control }
    entity registry "镜像仓库" { type: database }
    entity argo "Argo CD" { type: control }
    entity apiserver "K8s API Server" { type: control }
    entity scheduler "Scheduler" { type: control }
    entity kubelet "Kubelet" { type: control }
    entity deployment "Deployment" { type: control }
    entity pods "新 Pod 副本集" { type: boundary }
    entity ingress "Ingress Service" { type: boundary }
    entity monitor "监控告警" { type: boundary }

    developer -> ci "push release tag"
    ci -> registry "build and push image"
    ci --> developer "build passed"
    ci -> argo "update manifest"
    argo -> apiserver "apply new deployment spec"
    apiserver -> deployment "create new replica set"
    deployment -> scheduler "request new pods"
    scheduler -> kubelet "assign nodes"
    kubelet -> pods "pull image and start"
    pods --> kubelet "readiness probe ok"
    kubelet --> apiserver "pod ready"
    apiserver -> ingress "shift part of traffic"
    ingress --> pods "live requests"
    pods -> monitor "publish metrics"
    monitor --> argo "canary healthy"
    argo -> apiserver "scale down old pods"
    apiserver --> developer "rollout complete"
}
