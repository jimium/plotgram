// 经典线性链：A → B → C
// Mermaid 对照: graph LR; A[开始] --> B[处理] --> C[结束]
diagram flowchart {
    title: "线性流程"
    config {
        direction: left-to-right
    }

    entity start "开始" { type: start }
    entity process "处理" { type: process }
    entity end "结束" { type: end }

    start -> process
    process -> end
}
