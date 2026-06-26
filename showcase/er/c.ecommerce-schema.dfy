// 电商 ER 模型：多实体 + 多关系
// Mermaid 对照: 复杂 erDiagram
diagram er {
    title: "电商数据模型"

    entity customer "Customer" {
        type: database
        meta.pk: "id"
        meta.fields: "name\nemail"
    }
    entity address "Address" {
        type: database
        meta.pk: "id"
        meta.fk: "customer_id"
        meta.fields: "street\ncity"
    }
    entity product "Product" {
        type: database
        meta.pk: "id"
        meta.fields: "name\nprice"
    }
    entity category "Category" {
        type: database
        meta.pk: "id"
        meta.fields: "name"
    }
    entity cart "Cart" {
        type: database
        meta.pk: "id"
        meta.fk: "customer_id"
    }
    entity cart_item "CartItem" {
        type: database
        meta.pk: "id"
        meta.fields: "fk.cart_id\nfk.product_id\nqty"
    }
    entity order "Order" {
        type: database
        meta.pk: "id"
        meta.fk: "customer_id"
        meta.fields: "total\nstatus"
    }
    entity order_item "OrderItem" {
        type: database
        meta.pk: "id"
        meta.fields: "fk.order_id\nfk.product_id\nqty"
    }
    entity payment "Payment" {
        type: database
        meta.pk: "id"
        meta.fk: "order_id"
        meta.fields: "amount\nmethod"
    }
    entity shipment "Shipment" {
        type: database
        meta.pk: "id"
        meta.fk: "order_id"
        meta.fields: "carrier\ntracking_no"
    }

    customer -> address "拥有" { cardinality: "1:N" }
    customer -> cart "持有" { cardinality: "1:1" }
    customer -> order "下单" { cardinality: "1:N" }
    cart -> cart_item "包含" { cardinality: "1:N" }
    product -> cart_item "加入" { cardinality: "1:N" }
    product -> category "归属" { cardinality: "N:1" }
    order -> order_item "明细" { cardinality: "1:N" }
    product -> order_item "售出" { cardinality: "1:N" }
    order -> payment "支付" { cardinality: "1:1" }
    order -> shipment "发货" { cardinality: "1:1" }
}
