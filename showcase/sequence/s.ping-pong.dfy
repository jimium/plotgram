// Ping-Pong：双向消息交换
// Mermaid 对照: sequenceDiagram 经典 ping-pong 示例
diagram sequence {
    title: "Ping-Pong"

    entity alice "Alice" { type: actor }
    entity bob "Bob" { type: actor }

    alice -> bob "ping"
    bob --> alice "pong"
}
