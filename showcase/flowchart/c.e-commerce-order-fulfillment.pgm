// 电商订单履端到端流程：支付 → 仓储 → 物流三段
// Mermaid 对照: 复杂电商履约流程图，含 group 分段与回环
diagram flowchart {
    title: "电商订单履端到端流程"
    config {
        direction: top-to-bottom
    }

    group payment "支付阶段" {
        entity[start] place_order "下单"
        entity[process] risk_check "风控核查"
        entity[decision] risk_gate "风控通过？"
        entity[process] pay "执行支付"
        entity[decision] pay_gate "支付成功？"
        entity[process] retry_pay "重试支付"
        entity[process] cancel "订单取消"
    }

    group warehouse "仓储阶段" {
        entity[process] alloc "库存分配"
        entity[process] pick "拣货"
        entity[process] pack "打包"
        entity[decision] stock_gate "有库存？"
        entity[process] restock "补货"
        entity[process] qc "质检"
        entity[decision] qc_gate "质检通过？"
    }

    group logistics "物流阶段" {
        entity[process] dispatch "分拣出库"
        entity[process] ship "揽收发运"
        entity[process] transit "运输中"
        entity[process] deliver "派送"
        entity[decision] sign_gate "签收成功？"
        entity[process] retry_deliver "再次派送"
        entity[end] done "订单完成"
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
