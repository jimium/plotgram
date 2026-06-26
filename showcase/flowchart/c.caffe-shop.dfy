// 开咖啡店政务流程：多决策分支、交叉回环
// Mermaid 对照: 复杂 graph TD，大量菱形节点与条件边
diagram flowchart {
    title: "开咖啡店流程"
    config {
        direction: top-to-bottom
    }

    entity n0 "触发意图" { type: start }
    entity n1 "给前端提示进入开咖啡店流程" { type: process }
    entity n9 "最后完成赋值" { type: process }
    entity n11 "AssignNode" { type: process }
    entity n13 "AssignNode" { type: process }
    entity n16 "购物车:酒类商品零售许可证" { type: process }
    entity n18 "购物车加入营业执照事项" { type: process }
    entity n10 "ResponseNode" { type: end }
    entity n21 "ResponseNode" { type: end }
    entity n2 "提前告知" { type: decision }
    entity n4 "选择：是否申领了营业执照" { type: decision }
    entity n5 "选择：酒类商品" { type: decision }
    entity n6 "选择题:店铺所在位置" { type: decision }
    entity n7 "选择题：使用面300平方米" { type: decision }
    entity n8 "是否委托他人办理？" { type: decision }
    entity n14 "选择店铺产权类型" { type: decision }
    entity n15 "是否属于钢结构的户外招牌" { type: decision }
    entity n17 "选择题:法人类型" { type: decision }
    entity n19 "是否以登陆的法人身份进行申请" { type: decision }
    entity n3 "分支" { type: decision }

    n0 -> n1
    n1 -> n2
    n2 -> n3 "继续"
    n2 -> n21 "放弃"
    n3 -> n4 "个人"
    n3 -> n19 "法人"
    n4 -> n18 "否"
    n4 -> n5 "Other"
    n5 -> n16 "是"
    n5 -> n6 "Other"
    n6 -> n13 "沿街店铺"
    n6 -> n7 "商城内店铺"
    n7 -> n11 "否"
    n7 -> n8 "Other"
    n8 -> n9
    n9 -> n10
    n11 -> n8
    n13 -> n14
    n14 -> n15
    n15 -> n7
    n16 -> n17
    n17 -> n6
    n18 -> n5
    n19 -> n4 "是"
    n19 -> n5 "Other"
}
