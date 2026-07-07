// CI/CD 安全流水线：源码 → 安全扫描 → 部署三段
// Mermaid 对照: 复杂 DevSecOps 流水线，含 group 分段与回环
diagram flowchart {
    title: "CI/CD 安全流水线"
    config {
        direction: top-to-bottom
    }

    group source "源码阶段" {
        entity[start] commit "代码提交"
        entity[process] lint "静态检查"
        entity[decision] lint_gate "检查通过？"
        entity[process] build "构建镜像"
        entity[process] unit_test "单元测试"
        entity[decision] test_gate "测试通过？"
        entity[process] notify_dev "通知开发修复"
    }

    group security "安全扫描阶段" {
        entity[process] sast "SAST 静态扫描"
        entity[decision] sast_gate "SAST 通过？"
        entity[process] sca "依赖与许可证扫描"
        entity[decision] sca_gate "许可证合规？"
        entity[process] image_scan "镜像漏洞扫描"
        entity[decision] image_gate "镜像合规？"
        entity[process] sign "镜像签名"
        entity[process] security_review "安全评审"
    }

    group deploy "部署阶段" {
        entity[process] stage "预发部署"
        entity[process] smoke "冒烟测试"
        entity[decision] smoke_gate "冒烟通过？"
        entity[process] canary "金丝雀发布"
        entity[decision] canary_gate "金丝雀健康？"
        entity[process] prod "全量上线"
        entity[process] rollback "自动回滚"
        entity[end] done "发布完成"
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
