// OAuth 授权码流程
// Mermaid 对照: sequenceDiagram 多参与者授权流程
diagram sequence {
    title: "OAuth 授权码登录"

    entity user "用户" { type: actor }
    entity browser "浏览器" { type: boundary }
    entity auth "认证服务" { type: control }
    entity resource "资源服务" { type: control }

    user -> browser "点击登录"
    browser -> auth "重定向到授权页"
    user -> auth "输入凭证"
    auth --> browser "返回授权码"
    browser -> auth "用授权码换 Token"
    auth --> browser "返回 Access Token"
    browser -> resource "携带 Token 请求资源"
    resource --> browser "返回受保护数据"
}
