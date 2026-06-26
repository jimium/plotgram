# 企业场景需求设计

本目录存放 Drawify 面向企业客户（含国内银行/互联网与国际市场）的场景分析、能力规划与 POC 设计文档。

与 `product/`（产品愿景与功能清单）、`specs/`（语言与 AST 规范）、`architecture/`（技术实现）不同，本目录聚焦**企业落地路径**：数据源接入、规模化出图、合规与变更治理等需求设计。

## 文档索引

| 文档 | 说明 | 状态 |
|------|------|------|
| [规模化架构图战略](./scale-diagram-strategy.md) | 企业规模化出图总体战略、场景矩阵、能力清单、落地路径（国内银行/互联网） | draft |
| [国际市场企业服务机会](./international-market-opportunities.md) | 国际行业、地区、售卖形态、产品包装与进入优先级 | draft |
| [企业能力路线图](./capability-roadmap.md) | DSL / 解析 / Diff / 渲染 / API 功能提升与 P0–P2 排期 | draft |
| [K8s 可视化行业现状](./k8s-visualization-landscape.md) | Drawify 出现前的业界解法、痛点对比、客户话术 | draft |
| K8s Connector POC 详细规格 | 见 [scale-diagram-strategy.md §9](./scale-diagram-strategy.md#9-poc-链路详解k8s-connector--聚合规则--diff-报告) | draft |

## 后续计划补充的文档（占位）

以下主题将在后续迭代中拆分为独立文档：

- `connectors/` — 各数据源 Connector 规格（K8s、Terraform、OpenAPI、CMDB、APM）
- `compose-rules-spec.md` — 聚合规则引擎语法与语义
- `ast-builder-sdk.md` — 程序化 AST 构建 SDK 接口
- `enterprise-api.md` — Server API 企业版接口全集
- `bank-compliance-scenarios.md` — 银行合规、安全域、数据流向场景
- `internet-scale-scenarios.md` — 微服务治理、发布流水线、故障复盘场景
- `performance-as-product-capability.md` — 快渲染、PNG 归档、Architecture Compare 产品化
- `international-compliance-mapping.md` — SOC2 / GDPR / HIPAA 与 Drawify 能力映射（英文白皮书）

## 阅读建议

1. 先读 [规模化架构图战略](./scale-diagram-strategy.md) 了解整体定位与分层架构
2. 研发排期参考 [企业能力路线图](./capability-roadmap.md)（DSL / 解析 / 渲染 P0–P2）
3. 若需对外 POC 或内部评审，直接跳转 [K8s Connector POC](./scale-diagram-strategy.md#9-poc-链路详解k8s-connector--聚合规则--diff-报告)
4. 实现细节对照 `specs/ast-spec.md`（AST/Patch）与 `architecture/overview.md`（系统架构）
