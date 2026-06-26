// Enterprise data lineage platform across ingestion, governance, and analytics
// Mermaid mapping: complex architecture graph with domain groups and lineage services
diagram architecture {
    title: "企业数据血缘平台"

    group sources "数据源" {
        entity app_db "业务数据库" { type: database }
        entity crm "CRM 系统" { type: service }
        entity data_lake "对象存储湖仓" { type: storage }
        entity logs "应用日志" { type: service }
    }

    group ingest "采集与处理" {
        entity cdc "CDC Connector" { type: service }
        entity kafka "Kafka" { type: queue }
        entity flink "Flink" { type: service }
        entity batch "Batch ETL" { type: service }

        cdc -> kafka "change events"
        kafka -> flink "stream processing"
        kafka -> batch "replay data"
    }

    group governance "治理与血缘" {
        entity catalog "Metadata Catalog" { type: service }
        entity lineage "Lineage Engine" { type: service }
        entity quality "Data Quality Rules" { type: service }
        entity policy "Access Policy Engine" { type: service }

        quality -> catalog "quality status"
        catalog -> policy "classified assets"
    }

    group analytics "分析与消费" {
        entity warehouse "Data Warehouse" { type: database }
        entity semantic "Semantic Layer" { type: service }
        entity bi "BI Dashboard" { type: frontend }
        entity ml "Feature Store" { type: database }

        warehouse -> semantic
        semantic -> bi
        warehouse -> ml "feature export"
    }

    app_db -> cdc "binlog"
    crm -> cdc "entity sync"
    logs -> kafka "log stream"
    data_lake -> batch "raw files"
    flink -> warehouse "curated tables"
    batch -> warehouse "daily snapshot"

    cdc -> catalog "register source"
    flink -> lineage "job lineage"
    batch -> lineage "table lineage"
    warehouse -> quality "quality scan"
    policy -> bi "access control"
    lineage -> bi "column lineage"
}
