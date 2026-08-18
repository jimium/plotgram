# ADR-002: 废除 config 块，算法选项为自由 map

> 状态：accepted  
> 日期：2026-07-28  
> 关联：ADR-001

## 背景

V1 的 `config { }` 块将布局/路由等引擎参数收纳在一个额外嵌套层中。同时 language-spec 试图在规范中枚举每个算法的全部选项（§4.4/§4.5 大表），导致规范与实现强耦合、每次加参数都要改 spec。

## 决策

1. **废除 `config` 块**：`layout`、`edge_routing`、`theme`、`render_style`、`direction` 等全部提升为 diagram body 级属性（与 `title` 同级）。
2. **算法选项块是自由 map**：`layout: hierarchical { key: value ... }` 中 `{ }` 内的字段，DSL 解析器不枚举、不校验，解析为通用 `Map<String, Value>`。校验由引擎各算法在运行时自行负责（未知 key 产生警告，非解析错误）。

## 含义

- language-spec 不再维护算法选项表；只需定义 `<algorithm_config> ::= <atom> ["{" <option_pair>* "}"]` 的语法形式
- 新增/修改算法参数不需要改 DSL 规范，只需改引擎代码
- 解析器对选项块只做：key 必须是 identifier，value 必须是合法 attribute_value
- `config` 关键字从保留字列表中移除
- V1 中 `config` 块内的写法需迁移为 body 级属性

## 示例

```tautcore
diagram {
    profile: flowchart
    title: "用户登录"
    layout: hierarchical { direction: top-to-bottom, group_padding: 20 }
    edge_routing: orthogonal
    theme: common.clean-light

    node login { label: "登录" }
    node auth { label: "认证" }
    login -> auth
}
```
