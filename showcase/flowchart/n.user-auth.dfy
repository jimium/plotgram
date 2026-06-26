// 用户认证流程：多实体 + 响应箭头
// Mermaid 对照: graph TD + 多节点带标签连线
diagram flowchart {
    title: "用户认证流程"
    config {
        direction: top-to-bottom
    }

    entity client "移动客户端" { type: client }
    entity gateway "API 网关" {
        type: gateway
        status: healthy
    }
    entity auth "认证服务" {
        type: service
        owner: "安全团队"
    }
    entity db "用户数据库" { type: database }
    entity cache "Token 缓存" { type: cache }

    client -> gateway "HTTPS 请求"
    gateway -> auth "转发认证请求"
    auth -> db "查询用户信息"
    db --> auth "返回用户记录"
    auth -> cache "存储 Token"
    cache --> auth "返回缓存结果"
    auth --> gateway "认证结果"
    gateway --> client "响应"
}
