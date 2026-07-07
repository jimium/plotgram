// 订单审批：决策分支 + 回环
// Mermaid 对照: graph TD + 菱形决策节点
diagram flowchart {
    title: "订单审批流程"
    config {
        direction: top-to-bottom
    }

    entity[start] submit "提交订单"
    entity[process] review "经理审核"
    entity[process] approved "审批通过"
    entity[process] rejected "驳回修改"
    entity[end] done "完成"
    entity[decision] check "金额是否超限"
    entity[process] finance "财务复核"

    submit -> review
    review -> check
    check -> finance "是"
    check -> approved "否"
    finance -> approved "通过"
    finance -> rejected "驳回"
    approved -> done
    rejected -> submit "重新提交"
}
