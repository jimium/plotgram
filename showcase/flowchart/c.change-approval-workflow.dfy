// Change approval workflow for regulated production releases
// Mermaid mapping: complex approval flowchart with audit and rollback gates
diagram flowchart {
    title: "生产变更审批流程"
    config {
        direction: top-to-bottom
    }

    entity request "提交变更申请" { type: start }
    entity classify "识别变更类型" { type: process }
    entity risk_gate "高风险变更？" { type: decision }
    entity impact "补充影响评估" { type: process }
    entity test_evidence "附加测试证据" { type: process }
    entity security_review "安全评审" { type: process }
    entity arch_review "架构评审" { type: process }
    entity cab_gate "CAB 审批通过？" { type: decision }
    entity schedule "安排变更窗口" { type: process }
    entity deploy "执行生产发布" { type: process }
    entity verify "发布后验证" { type: process }
    entity verify_gate "验证通过？" { type: decision }
    entity rollback "执行回滚" { type: process }
    entity archive "归档审计材料" { type: process }
    entity reject "驳回并补充材料" { type: process }
    entity done "变更完成" { type: end }

    request -> classify
    classify -> risk_gate
    risk_gate -> impact "是"
    risk_gate -> test_evidence "否"
    impact -> test_evidence
    test_evidence -> security_review
    security_review -> arch_review
    arch_review -> cab_gate
    cab_gate -> schedule "是"
    cab_gate -> reject "否"
    reject -> request "补充后重提"
    schedule -> deploy
    deploy -> verify
    verify -> verify_gate
    verify_gate -> archive "是"
    verify_gate -> rollback "否"
    rollback -> archive
    archive -> done
}
