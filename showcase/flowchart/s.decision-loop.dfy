// 决策回环：认证失败时返回起点
// Mermaid 对照: graph LR + 菱形节点 + 条件边
diagram flowchart {
    title: "登录决策回环"
    config {
        direction: left-to-right
    }

    entity request "用户请求" { type: client }
    entity gateway "API 网关" { type: gateway }
    entity auth "认证服务" { type: service }
    entity db "数据库" { type: database }

    request -> gateway
    gateway -> auth
    auth -> db "成功"
    auth -> request "失败"
}
