// Node failure recovery sequence in a large K8s cluster
// Mermaid mapping: complex sequence diagram for failure detection and workload recovery
diagram sequence {
    title: "K8s 节点故障恢复链路"

    entity[boundary] monitor "监控系统"
    entity[control] apiserver "API Server"
    entity[control] controller "Node Controller"
    entity[control] scheduler "Scheduler"
    entity[control] autoscaler "Cluster Autoscaler"
    entity[control] kubelet "故障节点 Kubelet"
    entity[boundary] workloads "受影响工作负载"
    entity[boundary] new_node "新节点"
    entity[actor] oncall "值班工程师"

    kubelet --> apiserver "heartbeat timeout"
    apiserver -> controller "node status lost"
    controller -> monitor "emit node alert"
    monitor --> oncall "page triggered"
    controller -> workloads "mark pods unavailable"
    controller -> scheduler "reschedule pending pods"
    scheduler -> autoscaler "capacity insufficient"
    autoscaler -> new_node "provision replacement node"
    new_node --> autoscaler "node ready"
    autoscaler --> scheduler "capacity added"
    scheduler -> new_node "assign recovered pods"
    new_node -> workloads "pull images and start containers"
    workloads --> apiserver "readiness restored"
    apiserver --> monitor "service recovered"
    monitor --> oncall "incident mitigated"
}
