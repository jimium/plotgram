# Drawify 图标库设计规范

> 版本：0.1.0-draft | 状态：建库前约束文档
>
> 本文档定义节点装饰图标的资产格式、目录组织、`catalog` 元数据与验收标准。
> 实现模块：`crates/drawify-core/src/icons/`（模块已就绪，待 `lib.rs` 接入）

---

## 1. 定位与原则

### 1.1 图标 vs 节点形状

Drawify 已有两层视觉语义，图标库只负责**第二层**：

```
entity.type  →  NodeShape（圆柱、人形、菱形…）  →  节点整体轮廓
entity.icon  →  Icon glyph（MySQL、Kafka、用户…） →  节点内侧装饰
```

| 层级 | 来源 | 示例 | 职责 |
|------|------|------|------|
| 形状 | `attributes.standard.type` + StyleSheet | `database` → 圆柱体 | 表达「是什么类的东西」 |
| 图标 | `attributes.standard.icon`（或推断） | `mysql` → 数据库 glyph | 表达「具体是哪个技术/角色」 |

图标**不替代** `NodeShape`；二者叠加使用。

### 1.2 设计目标

| 目标 | 说明 |
|------|------|
| 排版组件 | 图标是节点内容的一部分，与标签横排排版，不是浮动贴纸 |
| 主题可继承 | 单色描边 + `currentColor`，随节点文字色 / 主题 token 变色 |
| 风格可共存 | 一套 glyph 资产适配全部 `graphic_style`（standard、excalidraw、blueprint…） |
| 编译期内嵌 | SVG 通过 `include_str!` 打包，WASM / CLI 无运行时文件 IO |
| 显式优先 | DSL `icon: mysql` 为主路径；标签推断仅作可选增强 |

### 1.3 默认布局策略

**内侧左对齐（icon + label 作为一组水平居中）** 为默认放置方式。

特殊 `NodeShape`（圆柱、人形、菱形等）第一期可不渲染图标，或在 catalog 中标记 `placement: none`。外侧角标变体留作后期扩展，不在本规范第一版资产范围内。

---

## 2. 目录结构

```
icons/
  README.md                 # 本文件
  mod.rs                    # 公共 API
  catalog.rs                # 注册表：id → IconDef
  resolve.rs                # entity → Option<&IconDef>
  render.rs                 # SVG 输出与内侧排版度量
  assets/
    glyphs/                 # 内侧主力资产（单色、24×24）
      people/
        user.svg
        actor.svg
        admin.svg
        team.svg
        bot.svg
      databases/
        database.svg          # 通用 fallback
        mysql.svg
        postgres.svg
        redis.svg
        mongodb.svg
        sqlite.svg
        elasticsearch.svg
        oracle.svg
        clickhouse.svg
        data_warehouse.svg
        data_lake.svg
      messaging/
        kafka.svg
        rabbitmq.svg
        eventbus.svg
        webhook.svg
      services/
        service.svg
        api.svg
        gateway.svg
        cache.svg
        queue.svg
        auth.svg
        worker.svg
        function.svg
        search.svg
        storage.svg
        cron.svg
        monitor.svg
        service_mesh.svg
        config.svg
        secret.svg
        ci.svg
        registry.svg
        spark.svg
        flink.svg
        argo.svg
        jenkins.svg
        prometheus.svg
        grafana.svg
        logs.svg
        traces.svg
        cdc.svg
        bi.svg
        ml_model.svg
        payment.svg
        ledger.svg
        notification.svg
        slack.svg
        jira.svg
        confluence.svg
        sentry.svg
        datadog.svg
      cloud/
        k8s.svg
        s3.svg
        lambda.svg
        docker.svg
        cdn.svg
        load_balancer.svg
        dns.svg
        waf.svg
        ingress.svg
        nginx.svg
        minio.svg
        pod.svg
        node.svg
        deployment.svg
      generic/
        server.svg
        external.svg
        browser.svg
        mobile.svg
        desktop.svg
        file.svg
        repo.svg
        pr.svg
        github.svg
        gitlab.svg
        folder.svg
        lock.svg
        globe.svg
```

### 2.1 目录规则

- **`glyphs/`**：符合本规范、可用于内侧排版的唯一正式资产目录。
- **按领域分子目录**：便于维护与贡献，但**视觉风格必须统一**（同一 grid、同一笔画体系）。
- **不在运行时扫描目录**：所有资产必须在 `catalog.rs`（或 build script 生成的清单）中显式注册。
- **`brands/`、`badges/`**：第一版不建；多色品牌 Logo、外侧角标底托留待 `placement: outside` 阶段。

### 2.2 预览页生成

新增或修改 `assets/glyphs/**/*.svg` 后，运行：

```bash
python3 crates/drawify-core/src/icons/assets/glyphs/generate_index.py
```

脚本会重建 `assets/glyphs/index.html`。该页面会内联 SVG，以便真实预览 `currentColor`、深色模式和不同主题色。

---

## 3. SVG 画布规范

### 3.1 基础参数

| 属性 | 值 | 说明 |
|------|-----|------|
| `viewBox` | `0 0 24 24` | 所有图标统一外接框，禁止各图标自定义 viewBox |
| 有效绘制区 | x/y: 3–21 | 每边留 3px 光学安全区，避免与文字贴挤 |
| 渲染尺寸 | `icon_size ≈ font_size` | 默认 `font_size × 1.0`，可在 catalog 用 `scale` 微调 |
| 宽高比 | 1:1（强制） | 宽 Logo、横标禁止直接入库；需做 compact glyph |

### 3.2 根元素模板

```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none">
  <path stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" d="..." />
</svg>
```

### 3.3 允许的元素

- `<path>`（首选）
- `<circle>`、`<rect>`、`<line>`、`<polyline>`、`<polygon>`（简单几何）
- `<g>` 分组（不含 transform 动画）

### 3.4 禁止的内容

| 禁止项 | 原因 |
|--------|------|
| 内嵌文字（`<text>`） | 缩放模糊、i18n 无意义 |
| `fill` 多色硬编码（`#xxx`） | 破坏主题继承 |
| 渐变（`<linearGradient>`）、滤镜（`<filter>`） | 导出与多风格兼容差 |
| `foreignObject`、位图 `<image>` | WASM 体积与一致性 |
| 根级 `width` / `height` 属性 | 由渲染器按 `icon_size` 缩放 |
| `stroke-width` 差异过大 | 同屏多图标视觉重量不一致；特殊项须在 catalog 注明 |

---

## 4. 笔画与视觉风格

### 4.1 默认笔画

| 属性 | 默认值 |
|------|--------|
| `stroke` | `currentColor` |
| `stroke-width` | `1.5` |
| `stroke-linecap` | `round` |
| `stroke-linejoin` | `round` |
| `fill` | `none`（默认）；允许少量 `fill="currentColor"` 实心区域 |

### 4.2 视觉重量

- 图标是「字体旁的符号」，不是插画；14–18px 渲染高度下必须可辨认。
- 优先使用**少而粗**的路径；避免 0.5px 级细节、密集平行线。
- 同目录内所有图标**光学面积相近**（不要有的撑满 24×24、有的只占中间 12×12）。

### 4.3 与背景的关系

图标落在节点 `fill` 色块上，必须保证在浅色主题底上可读：

- 依赖 `currentColor`（通常与节点文字色一致），不自行假设背景色。
- 避免大面积实心 `fill` 块与节点底色糊成一团；实心 glyph 优先用描边镂空风格。
- 验收时须在至少两种节点底色（浅绿 `service`、浅橙 `database`）上目测对比度。

### 4.4 品牌与技术图标

- 入库的是**语义符号**（如 Kafka 三道波纹、数据库圆柱简纹），不是官方彩色 Logo。
- 可参考 Simple Icons 等开源集的 path，但须简化为本规范笔画体系，并确认许可证（见 §8）。

---

## 5. 命名与 ID 规则

### 5.1 文件命名

- 小写 ASCII + 下划线：`mysql.svg`、`rabbit_mq.svg`（优先无下划线：`rabbitmq.svg`）
- 文件名 = catalog `id`
- 不使用版本后缀（`mysql_v2.svg` ❌）；迭代直接覆盖

### 5.2 ID 语义

| 类型 | 示例 | 说明 |
|------|------|------|
| 技术 | `mysql`, `postgres`, `redis`, `kafka` | 具体产品 |
| 角色 | `user`, `actor`, `admin` | 人物/角色 |
| 类别 fallback | `database`, `cache`, `queue`, `api` | 无具体技术匹配时的类别图标 |
| 通用 | `server`, `external`, `service` | 兜底 |

### 5.3 别名（aliases）

别名在 `catalog` 中声明，不通过重复文件实现：

```text
postgres  → aliases: [postgresql, pg]
k8s       → aliases: [kubernetes]
rabbitmq  → aliases: [rabbit_mq, amqp]
```

---

## 6. Catalog 字段规范

每个图标在 `catalog.rs` 中注册为 `IconDef`。第一版字段如下。

### 6.1 必填字段

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | `&'static str` | 唯一标识，与文件名一致 |
| `asset` | `&'static str` | `include_str!` 路径，相对 `assets/glyphs/` |
| `category` | `IconCategory` | 枚举：`People` / `Databases` / `Messaging` / `Services` / `Cloud` / `Generic` |
| `aliases` | `&'static [&'static str]` | 匹配用别名（可为空） |

### 6.2 布局相关字段

| 字段 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `placement` | `IconPlacement` | `Inside` | 第一版仅实现 `Inside`；`None` / `Outside` 预留 |
| `scale` | `f64` | `1.0` | 相对 `font_size` 的缩放比 |
| `optical_align_dy` | `f64` | `0.0` | 相对文字中心的垂直微调（px，正值下移） |
| `gap` | `f64` | `4.0` | 图标与标签文字间距（px） |
| `min_node_height` | `f64` | `32.0` | 节点高度低于此值时不渲染图标 |
| `min_node_width` | `f64` | `48.0` | 节点宽度低于此值时不渲染图标 |

### 6.3 兼容性与推断字段

| 字段 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `incompatible_shapes` | `&'static [NodeShape]` | `[]` | 这些形状上不显示（如 `Person`, `Cylinder`） |
| `fallback_for_types` | `&'static [&'static str]` | `[]` | 当 `entity.type` 匹配且无显式 `icon` 时使用 |
| `label_keywords` | `&'static [&'static str]` | `[]` | 标签推断关键词（小写）；低置信度可不启用 |
| `label_match_priority` | `u8` | `0` | 多关键词命中时的优先级（越大越优先） |

### 6.4 Rust 结构参考

```rust
pub enum IconPlacement {
    Inside,   // 内侧横排（默认，第一版实现）
    Outside,  // 外侧角标（预留）
    None,     // 注册但不渲染
}

pub enum IconCategory {
    People,
    Databases,
    Messaging,
    Services,
    Cloud,
    Generic,
}

pub struct IconDef {
    pub id: &'static str,
    pub asset: &'static str,
    pub category: IconCategory,
    pub aliases: &'static [&'static str],

    pub placement: IconPlacement,
    pub scale: f64,
    pub optical_align_dy: f64,
    pub gap: f64,
    pub min_node_width: f64,
    pub min_node_height: f64,

    pub incompatible_shapes: &'static [NodeShape],
    pub fallback_for_types: &'static [&'static str],
    pub label_keywords: &'static [&'static str],
    pub label_match_priority: u8,
}
```

### 6.5 注册示例

```rust
IconDef {
    id: "mysql",
    asset: include_str!("assets/glyphs/databases/mysql.svg"),
    category: IconCategory::Databases,
    aliases: &["mariadb"],
    placement: IconPlacement::Inside,
    scale: 1.0,
    optical_align_dy: 0.5,
    gap: 4.0,
    min_node_width: 56.0,
    min_node_height: 32.0,
    incompatible_shapes: &[NodeShape::Cylinder],
    fallback_for_types: &[],
    label_keywords: &["mysql"],
    label_match_priority: 10,
},
IconDef {
    id: "database",
    asset: include_str!("assets/glyphs/databases/database.svg"),
    category: IconCategory::Databases,
    aliases: &["db", "sql"],
    placement: IconPlacement::Inside,
    scale: 1.0,
    optical_align_dy: 0.0,
    gap: 4.0,
    min_node_width: 48.0,
    min_node_height: 32.0,
    incompatible_shapes: &[NodeShape::Cylinder],
    fallback_for_types: &["database"],
    label_keywords: &[],
    label_match_priority: 0,
},
```

---

## 7. 解析优先级

`resolve(entity) -> Option<IconId>` 按以下顺序匹配，命中即返回：

1. **显式属性** — `entity.attributes.standard.icon`
2. **entity id** — 规范化后精确匹配 catalog `id` 或 `aliases`
3. **type fallback** — `entity.type` 命中某图标的 `fallback_for_types`
4. **标签关键词** — `label_keywords` 子串匹配（可选，需全局开关；多命中取 `label_match_priority` 最高）

未命中则返回 `None`，不渲染图标。

形状兼容性：若当前 `NodeShape` 在 `incompatible_shapes` 中，即使命中 id 也**不渲染**。

---

## 8. 许可证与品牌

| 要求 | 说明 |
|------|------|
| 原创优先 | 第一版 MVP 优先自制简化 glyph |
| 第三方 path | 须记录来源与许可证（推荐 MIT / CC0 类，如 Simple Icons） |
| 官方 Logo | 不直接嵌入彩色商标；必要时仅作外侧 `brands/` 后期扩展 |
| 贡献说明 | 新增图标 PR 须注明出处；无出处视为原创贡献 |

在 `catalog` 或同级 `LICENSES.md`（后期）中维护第三方归属表。

---

## 9. 验收清单

每个图标入库前须逐项通过（可在 PR 模板中复用）。

### 9.1 格式

- [ ] `viewBox="0 0 24 24"`
- [ ] 无 `width` / `height` 根属性
- [ ] 无硬编码颜色（`#`、`rgb()`）
- [ ] 无 `<text>`、渐变、滤镜、`foreignObject`、位图
- [ ] `stroke="currentColor"` 或 `fill="currentColor"`（或二者组合）
- [ ] `stroke-width` 为 1.5（或 catalog 已注明例外）

### 9.2 视觉

- [ ] 16×16px 渲染可辨认
- [ ] 与 12px 字号标签并排时，`optical_align_dy` 已目测调平
- [ ] 有效内容在 3–21 光学安全区内
- [ ] 与同分类其他图标视觉重量接近
- [ ] 在浅色 `service` 底、浅色 `database` 底上可读

### 9.3 注册

- [ ] `catalog.rs` 已注册，`id` 与文件名一致
- [ ] `aliases` 覆盖常见写法（如 `postgres` / `postgresql` / `pg`）
- [ ] `incompatible_shapes` 已按形状语义设置
- [ ] 若为类别 fallback，已填 `fallback_for_types`
- [ ] `cargo test` / 图标模块单元测试通过（模块就绪后）

### 9.4 布局冒烟（模块就绪后）

- [ ] 短标签（≤4 字）+ 图标：整组水平居中，无偏斜
- [ ] 长标签（≥12 字）+ 图标：节点宽度正确扩展，文字不压图标
- [ ] `min_node_width` 以下节点不显示图标
- [ ] `incompatible_shapes` 形状上不显示图标

---

## 10. MVP 范围（第一版）

### 10.1 纳入

| 分类 | 建议图标 |
|------|----------|
| people | `user`, `actor`, `admin`, `team`, `bot` |
| databases | `database`, `mysql`, `postgres`, `redis`, `mongodb`, `sqlite`, `elasticsearch`, `oracle`, `clickhouse`, `data_warehouse`, `data_lake` |
| messaging | `kafka`, `rabbitmq`, `eventbus`, `webhook` |
| services | `service`, `api`, `gateway`, `cache`, `queue`, `auth`, `worker`, `function`, `search`, `storage`, `cron`, `monitor`, `service_mesh`, `config`, `secret`, `ci`, `registry`, `spark`, `flink`, `argo`, `jenkins`, `prometheus`, `grafana`, `logs`, `traces`, `cdc`, `bi`, `ml_model`, `payment`, `ledger`, `notification`, `slack`, `jira`, `confluence`, `sentry`, `datadog` |
| cloud | `k8s`, `s3`, `lambda`, `docker`, `cdn`, `load_balancer`, `dns`, `waf`, `ingress`, `nginx`, `minio`, `pod`, `node`, `deployment` |
| generic | `server`, `external`, `browser`, `mobile`, `desktop`, `file`, `repo`, `pr`, `github`, `gitlab`, `folder`, `lock`, `globe` |

合计约 **80+** 个 glyph。第一版仍优先覆盖架构图、流程图与 showcase 高频生产场景，不纳入多色品牌 Logo。

### 10.2 暂不纳入

- 多色品牌 `brands/`
- 外侧角标 `badges/`
- 按 `graphic_style` 分套的图标变体
- ER 表内图标、时序图参与者图标、思维导图叶子节点图标

### 10.3 适用图表

第一版仅保证 **architecture**、**flowchart** 矩形类节点可用。

---

## 11. 与 DSL / 管线的关系（预留）

```drawify
entity db "主库" {
    type: database
    icon: mysql
}
```

| 阶段 | 职责 |
|------|------|
| `validate` | `icon` 值须在 catalog 中存在（或为 `none`） |
| `layout` | `node_width += icon_size + gap`（有图标时） |
| `render` | `shape` + `icon` + `label` 组合输出 |

`icon_placement` 属性留待第二版；第一版由 catalog `placement` 内部决定。

---

## 12. 修订记录

| 版本 | 日期 | 说明 |
|------|------|------|
| 0.1.0-draft | 2026-06-10 | 初稿：内侧 glyph 规范、catalog 字段、验收清单 |
