// Settlement and reconciliation lifecycle for daily financial closing
// Mermaid mapping: complex state diagram for clearing and exception handling
diagram state {
    title: "清结算对账生命周期"

    entity[initial] init "初始化"
    entity[state] pending "Pending"
    entity[state] clearing "Clearing"
    entity[state] settling "Settling"
    entity[state] reconciling "Reconciling"
    entity[state] exception "Exception"
    entity[state] adjusting "Adjusting"
    entity[final] closed "Closed"
    entity[final] failed "Failed"
    entity[choice] gate "是否平账？"

    init -> pending
    pending -> clearing "receive cutoff batch"
    clearing -> settling "channel confirmed"
    settling -> reconciling "funds posted"
    reconciling -> gate
    gate -> closed "yes"
    gate -> exception "no"
    exception -> adjusting "manual adjustment"
    adjusting -> reconciling "re-run reconcile"
    settling -> failed "settlement rejected"
}
