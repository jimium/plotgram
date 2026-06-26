// 红绿灯状态机
// Mermaid 对照: stateDiagram-v2; [*]-->Red; Red-->Green; Green-->Yellow; Yellow-->Red
diagram state {
    title: "红绿灯"

    entity red "红灯" { type: state }
    entity green "绿灯" { type: state }
    entity yellow "黄灯" { type: state }

    red -> green "30s 后"
    green -> yellow "25s 后"
    yellow -> red "3s 后"
}
