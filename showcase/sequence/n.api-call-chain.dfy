// API 调用链：网关 → 服务 → 数据库
// Mermaid 对照: sequenceDiagram 多层服务调用
diagram sequence {
    title: "API 调用链"

    entity client "客户端" { type: boundary }
    entity gateway "API 网关" { type: boundary }
    entity user_svc "用户服务" { type: control }
    entity db "数据库" { type: database }

    client -> gateway "GET /users/me"
    gateway -> user_svc "转发请求"
    user_svc -> db "查询用户"
    db --> user_svc "返回记录"
    user_svc --> gateway "用户信息"
    gateway --> client "200 OK"
}
