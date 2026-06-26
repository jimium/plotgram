// 事件驱动架构：生产者-消费者模式
// Mermaid 对照: graph + subgraph + 异步消息流
diagram architecture {
    title: "事件驱动架构"

    group producers "生产者" {
        entity order_svc "订单服务" { type: service }
        entity user_svc "用户服务" { type: service }
    }

    group messaging "消息中间件" {
        entity kafka "Kafka" { type: queue }
    }

    group consumers "消费者" {
        entity analytics "分析服务" { type: service }
        entity notify "通知服务" { type: service }
        entity search "搜索服务" { type: service }
    }

    order_svc -> kafka "OrderCreated"
    user_svc -> kafka "UserRegistered"
    kafka --> analytics
    kafka --> notify
    kafka --> search
}
