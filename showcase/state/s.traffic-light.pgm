// 红绿灯状态机
// Mermaid 对照: stateDiagram-v2; [*]-->Red; Red-->Green; Green-->Yellow; Yellow-->Red
diagram state {
    title: "红绿灯"

    entity[state] red "红灯"
    entity[state] green "绿灯"
    entity[state] yellow "黄灯"

    red -> green "30s 后"
    green -> yellow "25s 后"
    yellow -> red "3s 后"
}
