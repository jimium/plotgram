// 决策回环：认证失败时返回起点
// Mermaid 对照: graph LR + 菱形节点 + 条件边
diagram flowchart {
    title: "登录决策回环"
    config {
        direction: left-to-right
    }

    entity[client] request "用户请求"
    entity[gateway] gateway "API 网关"
    entity[service] auth "认证服务"
    entity[database] db "数据库"

    request -> gateway
    gateway -> auth
    auth -> db "成功"
    auth -> request "失败"
}
