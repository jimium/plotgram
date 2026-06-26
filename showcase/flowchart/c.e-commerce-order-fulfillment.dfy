// 电商订单履端到端流程：支付 → 仓储 → 物流三段
// Mermaid 对照: 复杂电商履约流程图，含 group 分段与回环
diagram flowchart {
    title: "电商订单履端到端流程"
    config {
        direction: top-to-bottom
    }

    group payment "支付阶段" {
        entity place_order "下单" { type: start }
        entity risk_check "风控核查" { type: process }
        entity risk_gate "风控通过？" { type: decision }
        entity pay "执行支付" { type: process }
        entity pay_gate "支付成功？" { type: decision }
        entity retry_pay "重试支付" { type: process }
        entity cancel "订单取消" { type: process }
    }

    group warehouse "仓储阶段" {
        entity alloc "库存分配" { type: process }
        entity pick "拣货" { type: process }
        entity pack "打包" { type: process }
        entity stock_gate "有库存？" { type: decision }
        entity restock "补货" { type: process }
        entity qc "质检" { type: process }
        entity qc_gate "质检通过？" { type: decision }
    }

    group logistics "物流阶段" {
        entity dispatch "分拣出库" { type: process }
        entity ship "揽收发运" { type: process }
        entity transit "运输中" { type: process }
        entity deliver "派送" { type: process }
        entity sign_gate "签收成功？" { type: decision }
        entity retry_deliver "再次派送" { type: process }
        entity done "订单完成" { type: end }
    }

    place_order -> risk_check
    risk_check -> risk_gate
    risk_gate -> pay "是"
    risk_gate -> cancel "否"
    pay -> pay_gate
    pay_gate -> retry_pay "否"
    retry_pay -> pay_gate "再次校验"
    pay_gate -> alloc "是"

    alloc -> stock_gate
    stock_gate -> pick "是"
    stock_gate -> restock "否"
    restock -> pick "补货完成"
    pick -> pack
    pack -> qc
    qc -> qc_gate
    qc_gate -> dispatch "是"
    qc_gate -> pick "否，重新拣货"

    dispatch -> ship
    ship -> transit
    transit -> deliver
    deliver -> sign_gate
    sign_gate -> done "是"
    sign_gate -> retry_deliver "否"
    retry_deliver -> deliver "重新派送"
}
