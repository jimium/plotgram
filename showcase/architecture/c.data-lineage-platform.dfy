// Enterprise data lineage platform across ingestion, governance, and analytics
// Mermaid mapping: complex architecture graph with domain groups and lineage services
diagram architecture {
    title: "企业数据血缘平台"

    group sources "数据源" {
        entity[database] app_db "业务数据库"
        entity[service] crm "CRM 系统"
        entity[storage] data_lake "对象存储湖仓"
        entity[service] logs "应用日志"
    }

    group ingest "采集与处理" {
        entity[service] cdc "CDC Connector"
        entity[queue] kafka "Kafka"
        entity[service] flink "Flink"
        entity[service] batch "Batch ETL"

        cdc -> kafka "change events"
        kafka -> flink "stream processing"
        kafka -> batch "replay data"
    }

    group governance "治理与血缘" {
        entity[service] catalog "Metadata Catalog"
        entity[service] lineage "Lineage Engine"
        entity[service] quality "Data Quality Rules"
        entity[service] policy "Access Policy Engine"

        quality -> catalog "quality status"
        catalog -> policy "classified assets"
    }

    group analytics "分析与消费" {
        entity[database] warehouse "Data Warehouse"
        entity[service] semantic "Semantic Layer"
        entity[frontend] bi "BI Dashboard"
        entity[database] ml "Feature Store"

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
