// 订单审批：决策分支 + 回环
// Mermaid 对照: graph TD + 菱形决策节点
diagram flowchart {
    title: "订单审批流程"
    config {
        direction: top-to-bottom
    }

    entity submit "提交订单" { type: start }
    entity review "经理审核" { type: process }
    entity approved "审批通过" { type: process }
    entity rejected "驳回修改" { type: process }
    entity done "完成" { type: end }
    entity check "金额是否超限" { type: decision }
    entity finance "财务复核" { type: process }

    submit -> review
    review -> check
    check -> finance "是"
    check -> approved "否"
    finance -> approved "通过"
    finance -> rejected "驳回"
    approved -> done
    rejected -> submit "重新提交"
}
