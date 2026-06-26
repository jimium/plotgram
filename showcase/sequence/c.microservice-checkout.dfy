// 微服务结账：多服务协作 + 异步回调
// Mermaid 对照: 复杂 sequenceDiagram（多参与者、多轮交互）
diagram sequence {
    title: "微服务结账流程"

    entity user "用户" { type: actor }
    entity web "Web 前端" { type: boundary }
    entity gateway "API 网关" { type: boundary }
    entity order "订单服务" { type: control }
    entity inventory "库存服务" { type: control }
    entity payment "支付服务" { type: control }
    entity notify "通知服务" { type: control }
    entity mq "消息队列" {
        type: control
        semantic: kafka
    }

    user -> web "提交订单"
    web -> gateway "POST /checkout"
    gateway -> order "创建订单"
    order -> inventory "锁定库存"
    inventory --> order "锁定成功"
    order -> payment "发起支付"
    payment --> order "支付成功"
    order -> mq "发布订单完成事件"
    mq --> notify "消费事件"
    notify --> user "发送确认邮件"
    order --> gateway "订单结果"
    gateway --> web "结账完成"
    web --> user "显示成功页"
}
