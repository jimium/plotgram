// Service degradation lifecycle with mitigation and recovery paths
// Mermaid mapping: complex state diagram for resilience operations
diagram state {
    title: "服务降级生命周期"

    entity init "初始化" { type: initial }
    entity healthy "Healthy" { type: state }
    entity warning "Warning" { type: state }
    entity degraded "Degraded" { type: state }
    entity limited "Limited" { type: state }
    entity failover "Failover" { type: state }
    entity recovering "Recovering" { type: state }
    entity stable "Stable" { type: final }
    entity failed "Failed" { type: final }
    entity gate "恢复成功？" { type: choice }

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
