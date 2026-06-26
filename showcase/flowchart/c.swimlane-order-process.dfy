// 泳道图：跨部门订单处理流程
// group 水平排列（泳道），组内垂直布局
diagram flowchart {
    title: "跨部门订单处理（泳道图）"
    config {
        direction: top-to-bottom
        group_arrangement: horizontal
        group_align: left
        group_gap: 80
    }

    group customer "客户" {
        entity[start] place_order "下单"
        entity[end] receive_goods "收货"
    }

    group sales "销售" {
        entity[process] verify_order "审核订单"
        entity[process] confirm_order "确认订单"
    }

    group warehouse "仓库" {
        entity[process] pick_goods "拣货"
        entity[process] pack_goods "打包"
    }

    group shipping "物流" {
        entity[process] dispatch "发货"
        entity[process] deliver "配送"
    }

    place_order -> verify_order
    verify_order -> confirm_order
    confirm_order -> pick_goods
    pick_goods -> pack_goods
    pack_goods -> dispatch
    dispatch -> deliver
    deliver -> receive_goods
}
