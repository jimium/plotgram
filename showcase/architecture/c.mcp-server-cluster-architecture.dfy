// MCP 服务集群架构：客户端 → 入口 → 无状态服务 → 共享状态
// Mermaid 对照: flowchart LR 四组分层结构
diagram architecture {
    title: "MCP 服务集群架构"

    group clients "客户端" {
        entity[frontend] cursor "Cursor MCP"
        entity[frontend] studio "Studio"
    }

    group edge "入口" {
        entity[gateway] lb "LB / API Gateway"
    }

    group stateless "无状态（可水平扩展）" {
        entity[service] mcp_server_1 "mcp-server-1"
        entity[service] mcp_server_2 "mcp-server-2"
        entity[service] mcp_server_n "mcp-server-n"
    }

    group shared "共享状态" {
        entity[database] pg "Postgres\ndiagram + revision 元数据" {
            semantic: postgres
        }
        entity[cache] redis "Redis\n可选：锁 / 缓存" {
            semantic: redis
        }
        entity[storage] object_store "Object Store\n大 DSL / 渲染缓存" {
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
