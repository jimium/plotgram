// Payment gateway interaction sequence
// Business Scenario: E-commerce transactions, showing interaction between e-commerce platform, 3rd-party payment gateway, and bank system after order placement
// Mermaid Mapping: sequenceDiagram with multiple actors for authorization and payment
diagram sequence {
    title: "支付网关交互时序"

    entity[actor] user "用户"
    entity[boundary] shop "电商前端"
    entity[control] order_svc "订单服务"
    entity[control] pay_gw "第三方支付网关"
    entity[database] bank "银行系统"

    user -> shop "1. 点击支付"
    shop -> order_svc "2. 发起支付请求"
    order_svc -> pay_gw "3. 创建支付订单"
    pay_gw --> order_svc "4. 返回支付凭证(Token)"
    order_svc --> shop "5. 唤起支付收银台"
    user -> pay_gw "6. 确认支付并输入密码"
    pay_gw -> bank "7. 请求扣款"
    bank --> pay_gw "8. 扣款成功"
    pay_gw --> user "9. 展示支付成功页面"
    pay_gw -> order_svc "10. 异步通知支付结果"
    order_svc --> shop "11. 更新订单支付状态"
}
