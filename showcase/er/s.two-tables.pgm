// 最简 ER：两表外键关联
// Mermaid 对照: erDiagram; CUSTOMER ||--o{ ORDER : places
diagram er {
    title: "客户与订单"

    entity[database] customer "Customer" {
        meta.pk: "id"
        meta.fields: "name\nemail"
    }
    entity[database] order "Order" {
        meta.pk: "id"
        meta.fk: "customer_id"
        meta.fields: "total\nstatus"
    }

    customer -> order "下单" {
        cardinality: "1:N"
    }
}
