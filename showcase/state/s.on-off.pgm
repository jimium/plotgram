// 开关两态
// Mermaid 对照: stateDiagram-v2; [*]-->Off; Off-->On; On-->Off
diagram state {
    title: "开关状态"

    entity[initial] init ""
    entity[state] off "关闭"
    entity[state] on "开启"

    init -> off
    off -> on "按下开关"
    on -> off "再次按下"
}
