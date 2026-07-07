// Enterprise payment and clearing platform with real-time and batch settlement paths
// Mermaid mapping: complex architecture graph for financial transaction processing
diagram architecture {
    title: "支付清结算平台"

    group channels "支付渠道" {
        entity[external] merchant "商户接入"
        entity[frontend] cashier "收银台"
        entity[external] partner "外部支付机构"

        merchant -> cashier
    }

    group core_txn "交易核心" {
        entity[gateway] gateway "Payment Gateway"
        entity[service] order "Order Service"
        entity[service] ledger "Ledger Service"
        entity[service] routing "Routing Engine"
        entity[service] risk "Risk Decision Engine"

        gateway -> order "create payment"
        order -> routing "select channel"
        routing -> risk "risk check"
        gateway -> ledger "post accounting"
    }

    group clearing "清结算中心" {
        entity[service] clearing_engine "Clearing Engine"
        entity[service] settlement "Settlement Engine"
        entity[service] recon "Reconciliation Service"
        entity[service] dispute "Dispute Service"

        clearing_engine -> settlement
        settlement -> recon "daily reconcile"
        recon -> dispute "exception case"
    }

    group data_layer "账务与数据" {
        entity[database] txn_db "Transaction DB"
        entity[database] ledger_db "Ledger DB"
        entity[queue] mq "Event Bus"
        entity[database] warehouse "Finance Warehouse"
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
