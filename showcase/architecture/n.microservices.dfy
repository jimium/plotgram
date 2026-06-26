// 微服务架构：group 分组 + 跨组连线
// Mermaid 对照: graph + subgraph 前端/后端分组
diagram architecture {
    title: "微服务架构"

    group frontend "前端层" {
        entity web "Web 客户端" {
            type: frontend
            semantic: browser
        }
        entity mobile "移动客户端" {
            type: frontend
            semantic: mobile
        }
    }

    group backend "后端层" {
        entity gateway "API 网关" {
            type: gateway
            semantic: nginx
        }
        entity user_svc "用户服务" { type: service }
        entity order_svc "订单服务" { type: service }
    }

    entity db "PostgreSQL" {
        type: database
        semantic: postgres
    }
    entity mq "消息队列" {
        type: queue
        semantic: kafka
    }

    web -> gateway
    mobile -> gateway
    gateway -> user_svc
    gateway -> order_svc
    user_svc -> db
    order_svc -> db
    order_svc -> mq "发布订单事件"
    mq --> user_svc "消费事件"
}
