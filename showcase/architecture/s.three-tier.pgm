// 经典三层架构：Client → API → DB
// Mermaid 对照: graph LR 三层结构
diagram architecture {
    title: "三层架构"

    entity[frontend] client "客户端" {
        semantic: browser
    }
    entity[service] api "API 服务"
    entity[database] db "数据库" {
        semantic: postgres
    }

    client -> api "HTTP 请求"
    api -> db "SQL 查询"
    db --> api "查询结果"
    api --> client "JSON 响应"
}
