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
        entity[database] app_db "业务数据库"
        entity[service] log_server "日志服务器"
    }

    group process "数据计算层" {
        layout: fan-out
        entity[queue] kafka "消息队列(Kafka)" {
            semantic: kafka
        }
        entity[service] flink "流计算(Flink)" {
            semantic: flink
        }
        entity[service] spark "批处理(Spark)" {
            semantic: spark
        }
    }

    group storage "数据存储层" {
        layout: vertical
        entity[database] hive "数仓(Hive)" {
            semantic: hive
        }
        entity[database] clickhouse "OLAP引擎" {
            semantic: clickhouse
        }
    }

    entity[frontend] bi "BI可视化看板" {
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
