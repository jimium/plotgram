// TCP 连接状态机（简化版）
// Mermaid 对照: 复杂 stateDiagram-v2
diagram state {
    title: "TCP 连接状态"

    entity closed "CLOSED" { type: state }
    entity listen "LISTEN" { type: state }
    entity syn_sent "SYN_SENT" { type: state }
    entity syn_rcvd "SYN_RCVD" { type: state }
    entity established "ESTABLISHED" { type: state }
    entity fin_wait1 "FIN_WAIT_1" { type: state }
    entity fin_wait2 "FIN_WAIT_2" { type: state }
    entity close_wait "CLOSE_WAIT" { type: state }
    entity closing "CLOSING" { type: state }
    entity last_ack "LAST_ACK" { type: state }
    entity time_wait "TIME_WAIT" { type: state }

    closed -> listen "被动打开"
    closed -> syn_sent "主动打开"
    listen -> syn_rcvd "收到 SYN"
    syn_sent -> established "三次握手完成"
    syn_rcvd -> established "三次握手完成"
    established -> fin_wait1 "主动关闭"
    established -> close_wait "收到 FIN"
    fin_wait1 -> fin_wait2 "收到 ACK"
    fin_wait1 -> closing "同时关闭"
    fin_wait2 -> time_wait "收到 FIN"
    close_wait -> last_ack "发送 FIN"
    closing -> time_wait "收到 FIN"
    last_ack -> closed "收到 ACK"
    time_wait -> closed "2MSL 超时"
}
