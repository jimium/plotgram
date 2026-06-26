// 开关两态
// Mermaid 对照: stateDiagram-v2; [*]-->Off; Off-->On; On-->Off
diagram state {
    title: "开关状态"

    entity init "" { type: initial }
    entity off "关闭" { type: state }
    entity on "开启" { type: state }

    init -> off
    off -> on "按下开关"
    on -> off "再次按下"
}
