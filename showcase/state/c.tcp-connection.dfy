// TCP 连接状态机（简化版）
// Mermaid 对照: 复杂 stateDiagram-v2
diagram state {
    title: "TCP 连接状态"

    entity[state] closed "CLOSED"
    entity[state] listen "LISTEN"
    entity[state] syn_sent "SYN_SENT"
    entity[state] syn_rcvd "SYN_RCVD"
    entity[state] established "ESTABLISHED"
    entity[state] fin_wait1 "FIN_WAIT_1"
    entity[state] fin_wait2 "FIN_WAIT_2"
    entity[state] close_wait "CLOSE_WAIT"
    entity[state] closing "CLOSING"
    entity[state] last_ack "LAST_ACK"
    entity[state] time_wait "TIME_WAIT"

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
