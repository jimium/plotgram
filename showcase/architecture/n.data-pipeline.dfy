// Data warehouse ETL processing architecture
// Business Scenario: Data visualization and processing, illustrating the pipeline from data collection, streaming/batch processing to OLAP engine for BI
// Mermaid Mapping: graph + subgraph for different data processing layers
diagram architecture {
    title: "数据仓ETL处理架构"
    config {
        group_sizing: uniform
    }

    group source "数据源层" {
        layout: horizontal
        entity app_db "业务数据库" { type: database }
        entity log_server "日志服务器" { type: service }
    }

    group process "数据计算层" {
        layout: fan-out
        entity kafka "消息队列(Kafka)" {
            type: queue
            semantic: kafka
        }
        entity flink "流计算(Flink)" {
            type: service
            semantic: flink
        }
        entity spark "批处理(Spark)" {
            type: service
            semantic: spark
        }
    }

    group storage "数据存储层" {
        layout: vertical
        entity hive "数仓(Hive)" {
            type: database
            semantic: hive
        }
        entity clickhouse "OLAP引擎" {
            type: database
            semantic: clickhouse
        }
    }

    entity bi "BI可视化看板" {
        type: frontend
        semantic: grafana
    }

    app_db -> kafka "Binlog 同步"
    log_server -> kafka "用户日志收集"
    kafka -> flink "实时流消费"
    kafka -> spark "离线批量消费"
    spark -> hive "T+1 离线入库"
    flink -> clickhouse "实时指标计算"
    hive -> clickhouse "汇总宽表同步"
    clickhouse -> bi "多维分析与大屏查询"
}
