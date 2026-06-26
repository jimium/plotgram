// SaaS 多租户 ER 模型
// Mermaid 对照: 复杂 erDiagram（多租户）
diagram er {
    title: "SaaS 多租户模型"

    group core "核心身份" {
        entity tenant "Tenant" {
            type: database
            meta.pk: "id"
            meta.fields: "name\nplan"
        }
        entity user "User" {
            type: database
            meta.pk: "id"
            meta.fk: "tenant_id"
            meta.fields: "email\nstatus"
        }
        entity role "Role" {
            type: database
            meta.pk: "id"
            meta.fk: "tenant_id"
            meta.fields: "name"
        }
        entity permission "Permission" {
            type: database
            meta.pk: "id"
            meta.fields: "code"
        }
    }

    group rbac "权限关联" {
        entity user_role "UserRole" {
            type: database
            meta.pk: "id"
            meta.fields: "fk.user_id\nfk.role_id"
        }
        entity role_perm "RolePermission" {
            type: database
            meta.pk: "id"
            meta.fields: "fk.role_id\nfk.permission_id"
        }
    }

    group workspace "工作区" {
        entity project "Project" {
            type: database
            meta.pk: "id"
            meta.fk: "tenant_id"
            meta.fields: "name"
        }
        entity ws "Workspace" {
            type: database
            meta.pk: "id"
            meta.fk: "tenant_id"
            meta.fields: "name"
        }
        entity audit_log "AuditLog" {
            type: database
            meta.pk: "id"
            meta.fields: "fk.user_id\nfk.project_id\naction"
        }
    }

    tenant -> user "成员" { cardinality: "1:N" }
    tenant -> project "项目" { cardinality: "1:N" }
    tenant -> ws "空间" { cardinality: "1:N" }
    user -> user_role "担任" { cardinality: "1:N" }
    role -> user_role "分配" { cardinality: "1:N" }
    role -> role_perm "授权" { cardinality: "1:N" }
    permission -> role_perm "包含" { cardinality: "1:N" }
    ws -> project "归属" { cardinality: "1:N" }
    user -> audit_log "操作" { cardinality: "1:N" }
    project -> audit_log "记录" { cardinality: "1:N" }
}
