# xxx

```drawify
diagram flowchart {
    layout: top-to-bottom
    title: "生产变更审批流程"

    entity[start] request "提交变更申请"
    entity[process] classify "识别变更类型"
    entity[decision] risk_gate "高风险变更？"
    entity[process] impact "补充影响评估"
    entity[process] test_evidence "附加测试证据"
    entity[process] security_review "安全评审"
    entity[process] arch_review "架构评审"
    entity[decision] cab_gate "CAB 审批通过？"
    entity[process] schedule "安排变更窗口"
    entity[process] deploy "执行生产发布"
    entity[process] verify "发布后验证"
    entity[decision] verify_gate "验证通过？"
    entity[process] rollback "执行回滚"
    entity[process] archive "归档审计材料"
    entity[process] reject "驳回并补充材料"
    entity[end] done "变更完成"

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

```
