// Enterprise payment and clearing platform with real-time and batch settlement paths
// Mermaid mapping: complex architecture graph for financial transaction processing
diagram architecture {
    title: "支付清结算平台"

    group channels "支付渠道" {
        entity merchant "商户接入" { type: external }
        entity cashier "收银台" { type: frontend }
        entity partner "外部支付机构" { type: external }

        merchant -> cashier
    }

    group core_txn "交易核心" {
        entity gateway "Payment Gateway" { type: gateway }
        entity order "Order Service" { type: service }
        entity ledger "Ledger Service" { type: service }
        entity routing "Routing Engine" { type: service }
        entity risk "Risk Decision Engine" { type: service }

        gateway -> order "create payment"
        order -> routing "select channel"
        routing -> risk "risk check"
        gateway -> ledger "post accounting"
    }

    group clearing "清结算中心" {
        entity clearing_engine "Clearing Engine" { type: service }
        entity settlement "Settlement Engine" { type: service }
        entity recon "Reconciliation Service" { type: service }
        entity dispute "Dispute Service" { type: service }

        clearing_engine -> settlement
        settlement -> recon "daily reconcile"
        recon -> dispute "exception case"
    }

    group data_layer "账务与数据" {
        entity txn_db "Transaction DB" { type: database }
        entity ledger_db "Ledger DB" { type: database }
        entity mq "Event Bus" { type: queue }
        entity warehouse "Finance Warehouse" { type: database }
    }

    cashier -> gateway
    routing -> partner "submit payment"
    partner --> gateway "payment callback"
    ledger -> ledger_db
    order -> txn_db
    gateway -> mq "payment event"
    mq --> clearing_engine "clearing event"
    recon -> warehouse "daily report"
    ledger -> warehouse "account export"
}
