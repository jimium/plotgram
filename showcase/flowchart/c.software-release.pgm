// 软件发布流水线：多阶段门禁 + 回滚路径
// Mermaid 对照: 复杂 CI/CD 流程图
diagram flowchart {
    title: "软件发布流水线"
    config {
        direction: top-to-bottom
    }

    entity[start] commit "代码提交"
    entity[process] build "构建"
    entity[process] unit_test "单元测试"
    entity[process] integration "集成测试"
    entity[process] staging "预发部署"
    entity[process] canary "金丝雀发布"
    entity[process] prod "全量上线"
    entity[process] rollback "回滚"
    entity[end] done "发布完成"
    entity[decision] gate_build "构建通过？"
    entity[decision] gate_test "测试通过？"
    entity[decision] gate_staging "预发验证？"
    entity[decision] gate_canary "金丝雀健康？"
    entity[process] notify "通知开发修复"

    commit -> build
    build -> gate_build
    gate_build -> unit_test "是"
    gate_build -> notify "否"
    unit_test -> gate_test
    gate_test -> integration "是"
    gate_test -> notify "否"
    integration -> staging
    staging -> gate_staging
    gate_staging -> canary "是"
    gate_staging -> notify "否"
    canary -> gate_canary
    gate_canary -> prod "是"
    gate_canary -> rollback "否"
    prod -> done
    rollback -> notify
    notify -> commit "修复后重试"
}
