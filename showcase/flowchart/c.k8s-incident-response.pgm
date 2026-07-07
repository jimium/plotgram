// Incident response workflow for large K8s production clusters
// Mermaid mapping: complex troubleshooting flowchart with rollback and scaling branches
diagram flowchart {
    title: "K8s 生产集群故障响应流程"
    config {
        direction: top-to-bottom
    }

    entity[start] alert "告警触发"
    entity[process] identify "识别异常服务"
    entity[decision] classify "故障类型判断"
    entity[process] node_check "节点健康检查"
    entity[process] release_check "近期发布检查"
    entity[process] traffic_check "流量与依赖检查"
    entity[decision] node_gate "节点异常？"
    entity[decision] release_gate "发布导致？"
    entity[decision] traffic_gate "流量激增？"
    entity[process] cordon "隔离异常节点"
    entity[process] reschedule "迁移工作负载"
    entity[process] rollback "执行回滚"
    entity[process] freeze "冻结继续发布"
    entity[process] scale "扩容关键服务"
    entity[process] rate_limit "启用限流与降级"
    entity[process] dependency "通知依赖团队排查"
    entity[process] verify "恢复验证"
    entity[end] resolved "故障恢复"
    entity[process] postmortem "复盘与补救项"

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
