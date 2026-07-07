// Employee onboarding process: Cross-departmental collaboration
// Business Scenario: Office collaboration, where HR, IT, and Admin departments collaborate to complete onboarding for new employees
// Mermaid Mapping: graph TD with multi-branch and conditional checks
diagram flowchart {
    title: "员工入职协作流程"
    config {
        direction: top-to-bottom
    }

    entity[start] offer "接受 Offer"
    entity[process] hr_doc "HR收集材料"
    entity[process] sign "签订合同"
    entity[decision] check "材料是否齐全"
    entity[service] it_setup "IT开通账号"
    entity[service] admin_setup "行政分配工位"
    entity[process] welcome "入职培训"
    entity[end] done "完成入职"

    offer -> hr_doc
    hr_doc -> check
    check -> hr_doc "否，补充材料"
    check -> sign "是，通过审核"
    sign -> it_setup "分配系统权限"
    sign -> admin_setup "分配办公资源"
    it_setup -> welcome
    admin_setup -> welcome
    welcome -> done
}
