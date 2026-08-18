//! Icon catalog: static registry of all available icons.
//!
//! Migrated from V1 `icons/catalog.rs`. Simplified for V2:
//! - No label_keywords / label_match_priority (kind replaces semantic inference)
//! - incompatible_shapes uses `&str` (V2 shape names)

/// Icon category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconCategory {
    People,
    Databases,
    Messaging,
    Services,
    Cloud,
    Generic,
}

/// A single icon definition with embedded SVG asset.
#[derive(Debug, Clone, Copy)]
pub struct IconDef {
    pub id: &'static str,
    pub category: IconCategory,
    pub aliases: &'static [&'static str],
    pub svg_content: &'static str,
    /// Shapes incompatible with this icon (won't render inside these).
    pub incompatible_shapes: &'static [&'static str],
}

const INCOMP_CYLINDER: &[&str] = &["cylinder"];
const INCOMP_PERSON: &[&str] = &["person"];
const INCOMP_HEXAGON: &[&str] = &["hexagon"];
const INCOMP_DIAMOND: &[&str] = &["diamond"];

static ICONS: &[IconDef] = &[
    // ── people ──
    IconDef {
        id: "user",
        category: IconCategory::People,
        aliases: &["users", "person"],
        svg_content: include_str!("../../assets/glyphs/people/user.svg"),
        incompatible_shapes: INCOMP_PERSON,
    },
    IconDef {
        id: "actor",
        category: IconCategory::People,
        aliases: &["participant"],
        svg_content: include_str!("../../assets/glyphs/people/actor.svg"),
        incompatible_shapes: INCOMP_PERSON,
    },
    IconDef {
        id: "admin",
        category: IconCategory::People,
        aliases: &["administrator", "owner"],
        svg_content: include_str!("../../assets/glyphs/people/admin.svg"),
        incompatible_shapes: INCOMP_PERSON,
    },
    IconDef {
        id: "team",
        category: IconCategory::People,
        aliases: &["group", "org", "organization"],
        svg_content: include_str!("../../assets/glyphs/people/team.svg"),
        incompatible_shapes: INCOMP_PERSON,
    },
    IconDef {
        id: "bot",
        category: IconCategory::People,
        aliases: &["robot", "assistant", "agent"],
        svg_content: include_str!("../../assets/glyphs/people/bot.svg"),
        incompatible_shapes: &[],
    },
    // ── databases ──
    IconDef {
        id: "mysql",
        category: IconCategory::Databases,
        aliases: &["mariadb"],
        svg_content: include_str!("../../assets/glyphs/databases/mysql.svg"),
        incompatible_shapes: INCOMP_CYLINDER,
    },
    IconDef {
        id: "postgres",
        category: IconCategory::Databases,
        aliases: &["postgresql", "pg"],
        svg_content: include_str!("../../assets/glyphs/databases/postgres.svg"),
        incompatible_shapes: INCOMP_CYLINDER,
    },
    IconDef {
        id: "redis",
        category: IconCategory::Databases,
        aliases: &[],
        svg_content: include_str!("../../assets/glyphs/databases/redis.svg"),
        incompatible_shapes: INCOMP_CYLINDER,
    },
    IconDef {
        id: "mongodb",
        category: IconCategory::Databases,
        aliases: &["mongo"],
        svg_content: include_str!("../../assets/glyphs/databases/mongodb.svg"),
        incompatible_shapes: INCOMP_CYLINDER,
    },
    IconDef {
        id: "sqlite",
        category: IconCategory::Databases,
        aliases: &["sqlite3"],
        svg_content: include_str!("../../assets/glyphs/databases/sqlite.svg"),
        incompatible_shapes: INCOMP_CYLINDER,
    },
    IconDef {
        id: "elasticsearch",
        category: IconCategory::Databases,
        aliases: &["elastic", "es", "opensearch"],
        svg_content: include_str!("../../assets/glyphs/databases/elasticsearch.svg"),
        incompatible_shapes: INCOMP_CYLINDER,
    },
    IconDef {
        id: "oracle",
        category: IconCategory::Databases,
        aliases: &["oracle_db", "oracledb"],
        svg_content: include_str!("../../assets/glyphs/databases/oracle.svg"),
        incompatible_shapes: INCOMP_CYLINDER,
    },
    IconDef {
        id: "clickhouse",
        category: IconCategory::Databases,
        aliases: &["ch", "olap_engine"],
        svg_content: include_str!("../../assets/glyphs/databases/clickhouse.svg"),
        incompatible_shapes: INCOMP_CYLINDER,
    },
    IconDef {
        id: "data_warehouse",
        category: IconCategory::Databases,
        aliases: &["warehouse", "dwh", "olap", "hive"],
        svg_content: include_str!("../../assets/glyphs/databases/data_warehouse.svg"),
        incompatible_shapes: INCOMP_CYLINDER,
    },
    IconDef {
        id: "data_lake",
        category: IconCategory::Databases,
        aliases: &["lake", "lakehouse", "data_lakehouse"],
        svg_content: include_str!("../../assets/glyphs/databases/data_lake.svg"),
        incompatible_shapes: INCOMP_CYLINDER,
    },
    IconDef {
        id: "database",
        category: IconCategory::Databases,
        aliases: &["db", "sql"],
        svg_content: include_str!("../../assets/glyphs/databases/database.svg"),
        incompatible_shapes: INCOMP_CYLINDER,
    },
    // ── messaging ──
    IconDef {
        id: "kafka",
        category: IconCategory::Messaging,
        aliases: &["apache_kafka"],
        svg_content: include_str!("../../assets/glyphs/messaging/kafka.svg"),
        incompatible_shapes: &[],
    },
    IconDef {
        id: "rabbitmq",
        category: IconCategory::Messaging,
        aliases: &["rabbit_mq", "amqp"],
        svg_content: include_str!("../../assets/glyphs/messaging/rabbitmq.svg"),
        incompatible_shapes: &[],
    },
    IconDef {
        id: "eventbus",
        category: IconCategory::Messaging,
        aliases: &["event_bus", "events", "pubsub", "pub_sub"],
        svg_content: include_str!("../../assets/glyphs/messaging/eventbus.svg"),
        incompatible_shapes: &[],
    },
    IconDef {
        id: "webhook",
        category: IconCategory::Messaging,
        aliases: &["hook", "callback"],
        svg_content: include_str!("../../assets/glyphs/messaging/webhook.svg"),
        incompatible_shapes: &[],
    },
    // ── services ──
    IconDef {
        id: "service",
        category: IconCategory::Services,
        aliases: &["component", "module"],
        svg_content: include_str!("../../assets/glyphs/services/service.svg"),
        incompatible_shapes: &[],
    },
    IconDef {
        id: "api",
        category: IconCategory::Services,
        aliases: &["rest", "http", "graphql", "grpc"],
        svg_content: include_str!("../../assets/glyphs/services/api.svg"),
        incompatible_shapes: &[],
    },
    IconDef {
        id: "gateway",
        category: IconCategory::Services,
        aliases: &["proxy", "ingress", "edge"],
        svg_content: include_str!("../../assets/glyphs/services/gateway.svg"),
        incompatible_shapes: INCOMP_HEXAGON,
    },
    IconDef {
        id: "cache",
        category: IconCategory::Services,
        aliases: &["memcached"],
        svg_content: include_str!("../../assets/glyphs/services/cache.svg"),
        incompatible_shapes: INCOMP_DIAMOND,
    },
    IconDef {
        id: "queue",
        category: IconCategory::Services,
        aliases: &["mq", "message_queue"],
        svg_content: include_str!("../../assets/glyphs/services/queue.svg"),
        incompatible_shapes: &[],
    },
    IconDef {
        id: "auth",
        category: IconCategory::Services,
        aliases: &["iam", "oauth", "sso", "login"],
        svg_content: include_str!("../../assets/glyphs/services/auth.svg"),
        incompatible_shapes: &[],
    },
    IconDef {
        id: "worker",
        category: IconCategory::Services,
        aliases: &["job", "consumer", "processor"],
        svg_content: include_str!("../../assets/glyphs/services/worker.svg"),
        incompatible_shapes: &[],
    },
    IconDef {
        id: "function",
        category: IconCategory::Services,
        aliases: &["fn", "serverless"],
        svg_content: include_str!("../../assets/glyphs/services/function.svg"),
        incompatible_shapes: &[],
    },
    IconDef {
        id: "search",
        category: IconCategory::Services,
        aliases: &["index", "query"],
        svg_content: include_str!("../../assets/glyphs/services/search.svg"),
        incompatible_shapes: &[],
    },
    IconDef {
        id: "storage",
        category: IconCategory::Services,
        aliases: &["blob", "object_storage", "bucket"],
        svg_content: include_str!("../../assets/glyphs/services/storage.svg"),
        incompatible_shapes: &[],
    },
    IconDef {
        id: "cron",
        category: IconCategory::Services,
        aliases: &["scheduler", "schedule", "timer"],
        svg_content: include_str!("../../assets/glyphs/services/cron.svg"),
        incompatible_shapes: &[],
    },
    IconDef {
        id: "monitor",
        category: IconCategory::Services,
        aliases: &["metrics", "observability", "alert"],
        svg_content: include_str!("../../assets/glyphs/services/monitor.svg"),
        incompatible_shapes: &[],
    },
    IconDef {
        id: "service_mesh",
        category: IconCategory::Services,
        aliases: &["mesh", "istio", "linkerd"],
        svg_content: include_str!("../../assets/glyphs/services/service_mesh.svg"),
        incompatible_shapes: &[],
    },
    IconDef {
        id: "config",
        category: IconCategory::Services,
        aliases: &["configuration", "configmap", "settings"],
        svg_content: include_str!("../../assets/glyphs/services/config.svg"),
        incompatible_shapes: &[],
    },
    IconDef {
        id: "secret",
        category: IconCategory::Services,
        aliases: &["secrets", "key", "cert", "certificate"],
        svg_content: include_str!("../../assets/glyphs/services/secret.svg"),
        incompatible_shapes: &[],
    },
    IconDef {
        id: "ci",
        category: IconCategory::Services,
        aliases: &["pipeline", "build", "github_actions", "gitlab_ci"],
        svg_content: include_str!("../../assets/glyphs/services/ci.svg"),
        incompatible_shapes: &[],
    },
    IconDef {
        id: "registry",
        category: IconCategory::Services,
        aliases: &[
            "image_registry",
            "container_registry",
            "harbor",
            "ecr",
            "acr",
            "gcr",
        ],
        svg_content: include_str!("../../assets/glyphs/services/registry.svg"),
        incompatible_shapes: &[],
    },
    IconDef {
        id: "spark",
        category: IconCategory::Services,
        aliases: &["apache_spark", "batch_compute"],
        svg_content: include_str!("../../assets/glyphs/services/spark.svg"),
        incompatible_shapes: &[],
    },
    IconDef {
        id: "flink",
        category: IconCategory::Services,
        aliases: &["apache_flink", "stream_compute"],
        svg_content: include_str!("../../assets/glyphs/services/flink.svg"),
        incompatible_shapes: &[],
    },
    IconDef {
        id: "argo",
        category: IconCategory::Services,
        aliases: &["argocd", "argo_cd", "argo_rollouts", "rollouts"],
        svg_content: include_str!("../../assets/glyphs/services/argo.svg"),
        incompatible_shapes: &[],
    },
    IconDef {
        id: "jenkins",
        category: IconCategory::Services,
        aliases: &["jenkins_ci"],
        svg_content: include_str!("../../assets/glyphs/services/jenkins.svg"),
        incompatible_shapes: &[],
    },
    IconDef {
        id: "prometheus",
        category: IconCategory::Services,
        aliases: &["prom", "metrics_store"],
        svg_content: include_str!("../../assets/glyphs/services/prometheus.svg"),
        incompatible_shapes: &[],
    },
    IconDef {
        id: "grafana",
        category: IconCategory::Services,
        aliases: &["dashboard", "dashboards"],
        svg_content: include_str!("../../assets/glyphs/services/grafana.svg"),
        incompatible_shapes: &[],
    },
    IconDef {
        id: "logs",
        category: IconCategory::Services,
        aliases: &["log", "loki", "audit_log"],
        svg_content: include_str!("../../assets/glyphs/services/logs.svg"),
        incompatible_shapes: &[],
    },
    IconDef {
        id: "traces",
        category: IconCategory::Services,
        aliases: &["trace", "tracing", "tempo", "jaeger", "span"],
        svg_content: include_str!("../../assets/glyphs/services/traces.svg"),
        incompatible_shapes: &[],
    },
    IconDef {
        id: "cdc",
        category: IconCategory::Services,
        aliases: &["change_data_capture", "connector", "debezium", "binlog"],
        svg_content: include_str!("../../assets/glyphs/services/cdc.svg"),
        incompatible_shapes: &[],
    },
    IconDef {
        id: "bi",
        category: IconCategory::Services,
        aliases: &["business_intelligence", "report", "reporting"],
        svg_content: include_str!("../../assets/glyphs/services/bi.svg"),
        incompatible_shapes: &[],
    },
    IconDef {
        id: "ml_model",
        category: IconCategory::Services,
        aliases: &["model", "model_scoring", "ai_model", "ml"],
        svg_content: include_str!("../../assets/glyphs/services/ml_model.svg"),
        incompatible_shapes: &[],
    },
    IconDef {
        id: "payment",
        category: IconCategory::Services,
        aliases: &["pay", "checkout", "cashier", "card"],
        svg_content: include_str!("../../assets/glyphs/services/payment.svg"),
        incompatible_shapes: &[],
    },
    IconDef {
        id: "ledger",
        category: IconCategory::Services,
        aliases: &["accounting", "book", "journal"],
        svg_content: include_str!("../../assets/glyphs/services/ledger.svg"),
        incompatible_shapes: &[],
    },
    IconDef {
        id: "notification",
        category: IconCategory::Services,
        aliases: &["notify", "alerting", "sms", "email", "message"],
        svg_content: include_str!("../../assets/glyphs/services/notification.svg"),
        incompatible_shapes: &[],
    },
    IconDef {
        id: "slack",
        category: IconCategory::Services,
        aliases: &["chatops", "chat", "messaging_app"],
        svg_content: include_str!("../../assets/glyphs/services/slack.svg"),
        incompatible_shapes: &[],
    },
    IconDef {
        id: "jira",
        category: IconCategory::Services,
        aliases: &["issue_tracker", "ticket", "tickets"],
        svg_content: include_str!("../../assets/glyphs/services/jira.svg"),
        incompatible_shapes: &[],
    },
    IconDef {
        id: "confluence",
        category: IconCategory::Services,
        aliases: &["wiki", "knowledge_base", "docs"],
        svg_content: include_str!("../../assets/glyphs/services/confluence.svg"),
        incompatible_shapes: &[],
    },
    IconDef {
        id: "sentry",
        category: IconCategory::Services,
        aliases: &["error_tracking", "errors"],
        svg_content: include_str!("../../assets/glyphs/services/sentry.svg"),
        incompatible_shapes: &[],
    },
    IconDef {
        id: "datadog",
        category: IconCategory::Services,
        aliases: &["dd", "datadog_monitoring"],
        svg_content: include_str!("../../assets/glyphs/services/datadog.svg"),
        incompatible_shapes: &[],
    },
    // ── cloud ──
    IconDef {
        id: "k8s",
        category: IconCategory::Cloud,
        aliases: &["kubernetes"],
        svg_content: include_str!("../../assets/glyphs/cloud/k8s.svg"),
        incompatible_shapes: &[],
    },
    IconDef {
        id: "s3",
        category: IconCategory::Cloud,
        aliases: &["bucket", "object_store"],
        svg_content: include_str!("../../assets/glyphs/cloud/s3.svg"),
        incompatible_shapes: INCOMP_CYLINDER,
    },
    IconDef {
        id: "lambda",
        category: IconCategory::Cloud,
        aliases: &["aws_lambda", "cloud_function"],
        svg_content: include_str!("../../assets/glyphs/cloud/lambda.svg"),
        incompatible_shapes: &[],
    },
    IconDef {
        id: "docker",
        category: IconCategory::Cloud,
        aliases: &["container", "containers"],
        svg_content: include_str!("../../assets/glyphs/cloud/docker.svg"),
        incompatible_shapes: &[],
    },
    IconDef {
        id: "cdn",
        category: IconCategory::Cloud,
        aliases: &["edge_network"],
        svg_content: include_str!("../../assets/glyphs/cloud/cdn.svg"),
        incompatible_shapes: &[],
    },
    IconDef {
        id: "load_balancer",
        category: IconCategory::Cloud,
        aliases: &["lb", "elb", "alb", "loadbalancer"],
        svg_content: include_str!("../../assets/glyphs/cloud/load_balancer.svg"),
        incompatible_shapes: &[],
    },
    IconDef {
        id: "dns",
        category: IconCategory::Cloud,
        aliases: &["domain", "route53"],
        svg_content: include_str!("../../assets/glyphs/cloud/dns.svg"),
        incompatible_shapes: &[],
    },
    IconDef {
        id: "waf",
        category: IconCategory::Cloud,
        aliases: &["web_application_firewall", "firewall"],
        svg_content: include_str!("../../assets/glyphs/cloud/waf.svg"),
        incompatible_shapes: INCOMP_HEXAGON,
    },
    IconDef {
        id: "ingress",
        category: IconCategory::Cloud,
        aliases: &["ingress_gateway", "ingress_controller"],
        svg_content: include_str!("../../assets/glyphs/cloud/ingress.svg"),
        incompatible_shapes: INCOMP_HEXAGON,
    },
    IconDef {
        id: "nginx",
        category: IconCategory::Cloud,
        aliases: &["nginx_ingress", "nginx_proxy"],
        svg_content: include_str!("../../assets/glyphs/cloud/nginx.svg"),
        incompatible_shapes: INCOMP_HEXAGON,
    },
    IconDef {
        id: "minio",
        category: IconCategory::Cloud,
        aliases: &["min_io", "s3_compatible"],
        svg_content: include_str!("../../assets/glyphs/cloud/minio.svg"),
        incompatible_shapes: INCOMP_CYLINDER,
    },
    IconDef {
        id: "pod",
        category: IconCategory::Cloud,
        aliases: &["pods", "replicaset", "replica_set"],
        svg_content: include_str!("../../assets/glyphs/cloud/pod.svg"),
        incompatible_shapes: &[],
    },
    IconDef {
        id: "node",
        category: IconCategory::Cloud,
        aliases: &["nodes", "k8s_node", "worker_node", "kubelet"],
        svg_content: include_str!("../../assets/glyphs/cloud/node.svg"),
        incompatible_shapes: &[],
    },
    IconDef {
        id: "deployment",
        category: IconCategory::Cloud,
        aliases: &["deploy", "workload", "workloads", "rollout"],
        svg_content: include_str!("../../assets/glyphs/cloud/deployment.svg"),
        incompatible_shapes: &[],
    },
    // ── generic ──
    IconDef {
        id: "server",
        category: IconCategory::Generic,
        aliases: &["svc"],
        svg_content: include_str!("../../assets/glyphs/generic/server.svg"),
        incompatible_shapes: &[],
    },
    IconDef {
        id: "external",
        category: IconCategory::Generic,
        aliases: &["third_party", "3rd_party"],
        svg_content: include_str!("../../assets/glyphs/generic/external.svg"),
        incompatible_shapes: &[],
    },
    IconDef {
        id: "browser",
        category: IconCategory::Generic,
        aliases: &["web", "frontend"],
        svg_content: include_str!("../../assets/glyphs/generic/browser.svg"),
        incompatible_shapes: &[],
    },
    IconDef {
        id: "mobile",
        category: IconCategory::Generic,
        aliases: &["app", "ios", "android"],
        svg_content: include_str!("../../assets/glyphs/generic/mobile.svg"),
        incompatible_shapes: &[],
    },
    IconDef {
        id: "desktop",
        category: IconCategory::Generic,
        aliases: &["client"],
        svg_content: include_str!("../../assets/glyphs/generic/desktop.svg"),
        incompatible_shapes: &[],
    },
    IconDef {
        id: "file",
        category: IconCategory::Generic,
        aliases: &["document", "doc"],
        svg_content: include_str!("../../assets/glyphs/generic/file.svg"),
        incompatible_shapes: &[],
    },
    IconDef {
        id: "repo",
        category: IconCategory::Generic,
        aliases: &["repository", "git", "code_repo", "source_code"],
        svg_content: include_str!("../../assets/glyphs/generic/repo.svg"),
        incompatible_shapes: &[],
    },
    IconDef {
        id: "pr",
        category: IconCategory::Generic,
        aliases: &["pull_request", "merge_request", "mr", "review"],
        svg_content: include_str!("../../assets/glyphs/generic/pr.svg"),
        incompatible_shapes: &[],
    },
    IconDef {
        id: "github",
        category: IconCategory::Generic,
        aliases: &["gh", "github_actions"],
        svg_content: include_str!("../../assets/glyphs/generic/github.svg"),
        incompatible_shapes: &[],
    },
    IconDef {
        id: "gitlab",
        category: IconCategory::Generic,
        aliases: &["gitlab_ci"],
        svg_content: include_str!("../../assets/glyphs/generic/gitlab.svg"),
        incompatible_shapes: &[],
    },
    IconDef {
        id: "folder",
        category: IconCategory::Generic,
        aliases: &["directory", "dir"],
        svg_content: include_str!("../../assets/glyphs/generic/folder.svg"),
        incompatible_shapes: &[],
    },
    IconDef {
        id: "lock",
        category: IconCategory::Generic,
        aliases: &["secure", "security"],
        svg_content: include_str!("../../assets/glyphs/generic/lock.svg"),
        incompatible_shapes: &[],
    },
    IconDef {
        id: "globe",
        category: IconCategory::Generic,
        aliases: &["internet", "world"],
        svg_content: include_str!("../../assets/glyphs/generic/globe.svg"),
        incompatible_shapes: &[],
    },
];

// ─── Public API ──────────────────────────────────────────────────

/// All registered icons.
pub fn all_icons() -> &'static [IconDef] {
    ICONS
}

/// Look up by exact id.
pub fn icon_by_id(id: &str) -> Option<&'static IconDef> {
    ICONS.iter().find(|icon| icon.id == id)
}

/// Look up by id or alias (normalized).
pub fn icon_by_key(key: &str) -> Option<&'static IconDef> {
    let key = normalize_key(key);
    if key.is_empty() || key == "none" {
        return None;
    }
    icon_by_id(&key).or_else(|| {
        ICONS
            .iter()
            .find(|icon| icon.aliases.iter().any(|alias| normalize_key(alias) == key))
    })
}

/// Normalize icon key: lowercase, `-` → `_`, trim.
pub fn normalize_key(value: &str) -> String {
    value.trim().to_ascii_lowercase().replace('-', "_")
}
