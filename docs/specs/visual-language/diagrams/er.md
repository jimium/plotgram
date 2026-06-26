# ER 图 (er)

## 定位

描述**数据实体及其关系**：表/实体之间的关联、基数（一对多、多对多）与语义。

**核心问题**：*数据由哪些实体组成、如何关联？*

---

## 适用场景

| ✅ 适合 | ❌ 不适合 |
|---------|----------|
| 数据库 Schema 设计 | 服务调用拓扑（用 `architecture`） |
| 主外键关系、基数标注 | 业务流程步骤（用 `flowchart`） |
| 领域模型、多租户数据模型 | API 交互时序（用 `sequence`） |
| SaaS / 电商表结构 | 运行时状态迁移（用 `state`） |

---

## 语法入口

```drawify
diagram er {
    layout: top-to-bottom
    title: "用户与文章"

    entity user "User" {
        type: database
        meta.pk: "id"
    }
    entity post "Post" {
        type: database
        meta.pk: "id"
        meta.fk: "user_id"
    }

    user -> post "writes" {
        cardinality: "1:N"
    }
}
```

---

## 允许的实体 type

ER 图**不限制** entity type（`ER_PROFILE.entity_types` 为空）— 所有 entity 统一渲染为**矩形表框**。

**推荐写法**：
- `type: database` 表示持久化实体（与语言规范一致）
- 主键、外键、字段信息放在 `meta.` 命名空间（`meta.pk`, `meta.fk`, `meta.fields`）
- entity 的 `label` 用表名或领域名（如 `"User"`, `"OrderItem"`）

---

## 关系箭头约定

| 箭头 | 在 ER 图中的含义 | 视觉 |
|------|-----------------|------|
| `->` | 实体间关联（参照方向） | 直线 + 箭头 |
| `-->` | 较少使用 | 直线 |
| `<->` | 多对多关联 | 直线 + 双向箭头 |

**基数标注** — 在 relation 属性块中声明：

```drawify
user -> post "发表" {
    cardinality: "1:N"
}
```

- 格式建议 `"M:N"`（如 `"1:N"`, `"N:M"`, `"1:1"`）
- 不符合 `M:N` 格式会触发校验警告
- 关系标签写业务语义（`"拥有"`, `"属于"`, `"引用"`）

---

## 布局与视觉默认值

| 属性 | 默认值 | 说明 |
|------|--------|------|
| `layout-algo` | `sugiyama-v2` | 分层布局，主表在上、从表在下 |
| `edge-routing` | `straight` | 简洁直线关系 |
| `layout` | `top-to-bottom` | 层级方向 |
| 样式方案 | `builtin.blueprint` | 蓝图风格 |

---

## 分组 (group)

可用 group 按**模式（schema）或业务域**划分表集合：

```drawify
group core "核心业务" {
    entity user "User" { type: database }
    entity order "Order" { type: database }
}

group billing "计费" {
    entity invoice "Invoice" { type: database }
}
```

---

## 写作规范

1. **entity ID 用小写蛇形** — `order_item`，label 可用 PascalCase。
2. **主外键写进 `meta.`** — 渲染器可展示，程序化工具可消费。
3. **每条 relation 标注 `cardinality`** — `"1:N"` 比纯文字标签更精确。
4. **箭头方向遵循参照完整性** — 多的一方指向一的一方，或遵循团队约定并保持一致。
5. **不要用架构图 type** — `service`/`gateway` 在 ER 图中无语义。

---

## 实现状态

✅ 专属渲染器与 Sugiyama 布局已稳定；`meta.pk` / `meta.fk` / `meta.fields` 与 `cardinality` 属性均已支持。

---

## 示例

| 复杂度 | 路径 | 说明 |
|--------|------|------|
| 简单 | `showcase/er/s.user-post.dfy` | 两表一对多 |
| 简单 | `showcase/er/s.two-tables.dfy` | 基础双表 |
| 正常 | `showcase/er/n.blog-schema.dfy` | 博客 Schema |
| 复杂 | `showcase/er/c.ecommerce-schema.dfy` | 电商多表 |

---

## 参见

- [实体类型标准](../entity-types.md) — ER 图 type 约定
- [布局算法 — ER 图](../../../architecture/layout-algorithms.md)
- [架构图](./architecture.md) — 运行时组件 vs 数据模型
- [language-spec.md §5.4 meta 属性](../../language-spec.md)
