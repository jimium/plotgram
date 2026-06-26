// MCP 服务集群架构：客户端 → 入口 → 无状态服务 → 共享状态
// Mermaid 对照: flowchart LR 四组分层结构
diagram architecture {
    title: "MCP 服务集群架构"

    group clients "客户端" {
        entity cursor "Cursor MCP" {
            type: frontend
        }
        entity studio "Studio" {
            type: frontend
        }
    }

    group edge "入口" {
        entity lb "LB / API Gateway" {
            type: gateway
        }
    }

    group stateless "无状态（可水平扩展）" {
        entity mcp_server_1 "mcp-server-1" { type: service }
        entity mcp_server_2 "mcp-server-2" { type: service }
        entity mcp_server_n "mcp-server-n" { type: service }
    }

    group shared "共享状态" {
        entity pg "Postgres\ndiagram + revision 元数据" {
            type: database
            semantic: postgres
        }
        entity redis "Redis\n可选：锁 / 缓存" {
            type: cache
            semantic: redis
        }
        entity object_store "Object Store\n大 DSL / 渲染缓存" {
            type: storage
            semantic: s3
        }
    }

    cursor -> lb
    studio -> lb
    lb -> mcp_server_1
    lb -> mcp_server_2
    lb -> mcp_server_n
    mcp_server_1 -> pg
    mcp_server_2 -> pg
    mcp_server_n -> pg
    mcp_server_1 -> redis
    mcp_server_2 -> redis
    mcp_server_1 -> object_store
}
