// 工单流转状态机
// Mermaid 对照: stateDiagram-v2 工单处理流程
diagram state {
    title: "工单流转"

    entity[initial] init ""
    entity[state] open "待处理"
    entity[state] assigned "已分配"
    entity[state] in_progress "处理中"
    entity[state] pending "等待用户"
    entity[state] resolved "已解决"
    entity[final] closed "已关闭"
    entity[state] reopened "重新打开"

    init -> open
    open -> assigned "分配工程师"
    assigned -> in_progress "开始处理"
    in_progress -> pending "需要用户反馈"
    pending -> in_progress "用户已回复"
    in_progress -> resolved "问题解决"
    resolved -> closed "用户确认"
    resolved -> reopened "用户不满意"
    reopened -> assigned
}
