// 微服务结账：多服务协作 + 异步回调
// Mermaid 对照: 复杂 sequenceDiagram（多参与者、多轮交互）
diagram sequence {
    title: "微服务结账流程"

    entity[actor] user "用户"
    entity[boundary] web "Web 前端"
    entity[boundary] gateway "API 网关"
    entity[control] order "订单服务"
    entity[control] inventory "库存服务"
    entity[control] payment "支付服务"
    entity[control] notify "通知服务"
    entity[control] mq "消息队列" {
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
