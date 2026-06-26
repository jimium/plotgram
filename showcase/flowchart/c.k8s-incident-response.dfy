// Incident response workflow for large K8s production clusters
// Mermaid mapping: complex troubleshooting flowchart with rollback and scaling branches
diagram flowchart {
    title: "K8s 生产集群故障响应流程"
    config {
        direction: top-to-bottom
    }

    entity alert "告警触发" { type: start }
    entity identify "识别异常服务" { type: process }
    entity classify "故障类型判断" { type: decision }
    entity node_check "节点健康检查" { type: process }
    entity release_check "近期发布检查" { type: process }
    entity traffic_check "流量与依赖检查" { type: process }
    entity node_gate "节点异常？" { type: decision }
    entity release_gate "发布导致？" { type: decision }
    entity traffic_gate "流量激增？" { type: decision }
    entity cordon "隔离异常节点" { type: process }
    entity reschedule "迁移工作负载" { type: process }
    entity rollback "执行回滚" { type: process }
    entity freeze "冻结继续发布" { type: process }
    entity scale "扩容关键服务" { type: process }
    entity rate_limit "启用限流与降级" { type: process }
    entity dependency "通知依赖团队排查" { type: process }
    entity verify "恢复验证" { type: process }
    entity resolved "故障恢复" { type: end }
    entity postmortem "复盘与补救项" { type: process }

    alert -> identify
    identify -> classify
    classify -> node_check "基础设施告警"
    classify -> release_check "发布后异常"
    classify -> traffic_check "延迟与错误率上升"

    node_check -> node_gate
    node_gate -> cordon "是"
    node_gate -> traffic_check "否"
    cordon -> reschedule
    reschedule -> verify

    release_check -> release_gate
    release_gate -> rollback "是"
    release_gate -> traffic_check "否"
    rollback -> freeze
    freeze -> verify

    traffic_check -> traffic_gate
    traffic_gate -> scale "是"
    traffic_gate -> dependency "否"
    scale -> rate_limit
    rate_limit -> verify
    dependency -> verify

    verify -> resolved "恢复完成"
    resolved -> postmortem
    postmortem -> alert "跟踪改进项"
}
