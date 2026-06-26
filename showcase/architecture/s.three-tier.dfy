// 经典三层架构：Client → API → DB
// Mermaid 对照: graph LR 三层结构
diagram architecture {
    title: "三层架构"

    entity client "客户端" {
        type: frontend
        semantic: browser
    }
    entity api "API 服务" { type: service }
    entity db "数据库" {
        type: database
        semantic: postgres
    }

    client -> api "HTTP 请求"
    api -> db "SQL 查询"
    db --> api "查询结果"
    api --> client "JSON 响应"
}
