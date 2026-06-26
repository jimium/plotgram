// Settlement and reconciliation lifecycle for daily financial closing
// Mermaid mapping: complex state diagram for clearing and exception handling
diagram state {
    title: "清结算对账生命周期"

    entity init "初始化" { type: initial }
    entity pending "Pending" { type: state }
    entity clearing "Clearing" { type: state }
    entity settling "Settling" { type: state }
    entity reconciling "Reconciling" { type: state }
    entity exception "Exception" { type: state }
    entity adjusting "Adjusting" { type: state }
    entity closed "Closed" { type: final }
    entity failed "Failed" { type: final }
    entity gate "是否平账？" { type: choice }

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
