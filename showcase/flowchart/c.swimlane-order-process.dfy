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
        entity place_order "下单" { type: start }
        entity receive_goods "收货" { type: end }
    }

    group sales "销售" {
        entity verify_order "审核订单" { type: process }
        entity confirm_order "确认订单" { type: process }
    }

    group warehouse "仓库" {
        entity pick_goods "拣货" { type: process }
        entity pack_goods "打包" { type: process }
    }

    group shipping "物流" {
        entity dispatch "发货" { type: process }
        entity deliver "配送" { type: process }
    }

    place_order -> verify_order
    verify_order -> confirm_order
    confirm_order -> pick_goods
    pick_goods -> pack_goods
    pack_goods -> dispatch
    dispatch -> deliver
    deliver -> receive_goods
}
