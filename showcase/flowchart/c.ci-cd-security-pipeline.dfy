// CI/CD 安全流水线：源码 → 安全扫描 → 部署三段
// Mermaid 对照: 复杂 DevSecOps 流水线，含 group 分段与回环
diagram flowchart {
    title: "CI/CD 安全流水线"
    config {
        direction: top-to-bottom
    }

    group source "源码阶段" {
        entity commit "代码提交" { type: start }
        entity lint "静态检查" { type: process }
        entity lint_gate "检查通过？" { type: decision }
        entity build "构建镜像" { type: process }
        entity unit_test "单元测试" { type: process }
        entity test_gate "测试通过？" { type: decision }
        entity notify_dev "通知开发修复" { type: process }
    }

    group security "安全扫描阶段" {
        entity sast "SAST 静态扫描" { type: process }
        entity sast_gate "SAST 通过？" { type: decision }
        entity sca "依赖与许可证扫描" { type: process }
        entity sca_gate "许可证合规？" { type: decision }
        entity image_scan "镜像漏洞扫描" { type: process }
        entity image_gate "镜像合规？" { type: decision }
        entity sign "镜像签名" { type: process }
        entity security_review "安全评审" { type: process }
    }

    group deploy "部署阶段" {
        entity stage "预发部署" { type: process }
        entity smoke "冒烟测试" { type: process }
        entity smoke_gate "冒烟通过？" { type: decision }
        entity canary "金丝雀发布" { type: process }
        entity canary_gate "金丝雀健康？" { type: decision }
        entity prod "全量上线" { type: process }
        entity rollback "自动回滚" { type: process }
        entity done "发布完成" { type: end }
    }

    commit -> lint
    lint -> lint_gate
    lint_gate -> build "是"
    lint_gate -> notify_dev "否"
    build -> unit_test
    unit_test -> test_gate
    test_gate -> sast "是"
    test_gate -> notify_dev "否"

    sast -> sast_gate
    sast_gate -> sca "是"
    sast_gate -> security_review "否，高危待评审"
    sca -> sca_gate
    sca_gate -> image_scan "是"
    sca_gate -> security_review "否"
    image_scan -> image_gate
    image_gate -> sign "是"
    image_gate -> security_review "否"
    sign -> stage
    security_review -> commit "评审后修复重提"

    stage -> smoke
    smoke -> smoke_gate
    smoke_gate -> canary "是"
    smoke_gate -> rollback "否"
    canary -> canary_gate
    canary_gate -> prod "是"
    canary_gate -> rollback "否"
    prod -> done
    rollback -> notify_dev
    notify_dev -> commit "修复后重试"
}
