// Rollout lifecycle with pause, promotion, and rollback
// Mermaid mapping: complex state diagram for progressive delivery
diagram state {
    title: "K8s 发布状态机"

    entity[initial] init "初始化"
    entity[state] created "Created"
    entity[state] progressing "Progressing"
    entity[state] paused "Paused"
    entity[state] verifying "Verifying"
    entity[state] promoted "Promoted"
    entity[state] degraded "Degraded"
    entity[state] rollback "RollingBack"
    entity[final] completed "Completed"
    entity[final] failed "Failed"
    entity[choice] gate "健康检查通过？"

    init -> created
    created -> progressing "开始发布"
    progressing -> paused "等待人工确认"
    progressing -> verifying "自动进入验证"
    paused -> verifying "继续发布"
    verifying -> gate
    gate -> promoted "通过"
    gate -> degraded "失败"
    promoted -> completed "全量切流完成"
    degraded -> rollback "触发回滚"
    rollback -> progressing "回退到稳定版本"
    rollback -> failed "回滚失败"
}
