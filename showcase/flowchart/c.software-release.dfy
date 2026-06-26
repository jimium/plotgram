// 软件发布流水线：多阶段门禁 + 回滚路径
// Mermaid 对照: 复杂 CI/CD 流程图
diagram flowchart {
    title: "软件发布流水线"
    config {
        direction: top-to-bottom
    }

    entity commit "代码提交" { type: start }
    entity build "构建" { type: process }
    entity unit_test "单元测试" { type: process }
    entity integration "集成测试" { type: process }
    entity staging "预发部署" { type: process }
    entity canary "金丝雀发布" { type: process }
    entity prod "全量上线" { type: process }
    entity rollback "回滚" { type: process }
    entity done "发布完成" { type: end }
    entity gate_build "构建通过？" { type: decision }
    entity gate_test "测试通过？" { type: decision }
    entity gate_staging "预发验证？" { type: decision }
    entity gate_canary "金丝雀健康？" { type: decision }
    entity notify "通知开发修复" { type: process }

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
