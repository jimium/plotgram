// 微服务架构：group 分组 + 跨组连线
// Mermaid 对照: graph + subgraph 前端/后端分组
diagram architecture {
    title: "微服务架构"

    group frontend "前端层" {
        entity[frontend] web "Web 客户端" {
            semantic: browser
        }
        entity[frontend] mobile "移动客户端" {
            semantic: mobile
        }
    }

    group backend "后端层" {
        entity[gateway] gateway "API 网关" {
            semantic: nginx
        }
        entity[service] user_svc "用户服务"
        entity[service] order_svc "订单服务"
    }

    entity[database] db "PostgreSQL" {
        semantic: postgres
    }
    entity[queue] mq "消息队列" {
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
