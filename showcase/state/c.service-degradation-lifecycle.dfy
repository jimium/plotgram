// Service degradation lifecycle with mitigation and recovery paths
// Mermaid mapping: complex state diagram for resilience operations
diagram state {
    title: "服务降级生命周期"

    entity[initial] init "初始化"
    entity[state] healthy "Healthy"
    entity[state] warning "Warning"
    entity[state] degraded "Degraded"
    entity[state] limited "Limited"
    entity[state] failover "Failover"
    entity[state] recovering "Recovering"
    entity[final] stable "Stable"
    entity[final] failed "Failed"
    entity[choice] gate "恢复成功？"

    init -> healthy
    healthy -> warning "latency rising"
    warning -> degraded "error budget exhausted"
    warning -> healthy "noise cleared"
    degraded -> limited "enable partial features"
    degraded -> failover "switch dependency"
    limited -> recovering "mitigation applied"
    failover -> recovering "traffic moved"
    recovering -> gate
    gate -> healthy "yes"
    gate -> failed "no"
    healthy -> stable "stable observation window"
}
