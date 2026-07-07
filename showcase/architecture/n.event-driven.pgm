// 事件驱动架构：生产者-消费者模式
// Mermaid 对照: graph + subgraph + 异步消息流
diagram architecture {
    title: "事件驱动架构"

    group producers "生产者" {
        entity[service] order_svc "订单服务"
        entity[service] user_svc "用户服务"
    }

    group messaging "消息中间件" {
        entity[queue] kafka "Kafka"
    }

    group consumers "消费者" {
        entity[service] analytics "分析服务"
        entity[service] notify "通知服务"
        entity[service] search "搜索服务"
    }

    order_svc -> kafka "OrderCreated"
    user_svc -> kafka "UserRegistered"
    kafka --> analytics
    kafka --> notify
    kafka --> search
}
