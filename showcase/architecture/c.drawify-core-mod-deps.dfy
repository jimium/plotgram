// drawify-core 内部 mod 依赖关系（编译期 DAG）
// 说明：箭头表示「use / 依赖」方向（A -> B 表示 A 依赖 B）
diagram architecture {
    title: "drawify-core 模块依赖图"
    config {
        theme: common.blueprint
        render_style: blueprint
    }

  // ── 第 0 层：叶子基础 ─────────────────────────────────────
    group l0 "L0 基础（几乎不依赖业务 mod）" {
        layout: horizontal
        entity ast "ast"
        entity error "error"
        entity types "types\ndiagram_type · standard_attr_keys · style_attr_keys · style_attrs"
    }

  // ── 第 1 层：契约 ─────────────────────────────────────────
    group l1 "L1 契约" {
        layout: horizontal
        entity profile "profile\nDiagramProfile · profile_for"
    }

  // ── 第 2 层：主题数据 + 选择策略 ─────────────────────────
    group l2 "L2 主题 theme" {
        layout: horizontal
        entity theme_data "theme/\nbuiltin · loader · resolve · cascade"
        entity theme_select "theme/select\nThemeIdResolver"
    }

  // ── 第 3 层：解析与预处理 ─────────────────────────────────
    group l3 "L3 解析 / 预处理" {
        layout: horizontal
        entity dsl "dsl\nlexer · parser"
        entity prepare "prepare\nprofile_defaults · styles"
        entity validation "validation\nattrs · common"
        entity layout "layout\n算法 · 路由"
    }

  // ── 第 4 层：图表行为实现 ─────────────────────────────────
    group l4 "L4 图表种类 kinds" {
        layout: horizontal
        entity dt_registry "registry\nvalidate + scene 分派"
        entity dt_flowchart "flowchart\nvalidate"
        entity dt_arch "architecture\nvalidate"
        entity dt_other "sequence · state\ner/semantics · mindmap\nmindmap_theme"
    }

  // ── 第 5 层：渲染与导出 ───────────────────────────────────
    group l5 "L5 渲染管线" {
        layout: horizontal
        entity graphic_style "graphic_style\n笔触皮肤"
        entity render "render\nscene · paint · encode\nRenderRequest"
    }

  // ── 第 6 层：对外入口 ─────────────────────────────────────
    group l6 "L6 入口" {
        layout: horizontal
        entity pipeline "pipeline\nparse · prepare"
        entity facade "facade"
    }

  // ── 反例：若 ENTITY_TYPES 放进 kinds 会成环 ───────
    group anti "❌ 禁止的依赖（会 mod 循环）" {
        layout: horizontal
        entity bad_registry "profile\n想 use kinds::flowchart"
        entity bad_flowchart "kinds/flowchart\nvalidate 已 use layout"
    }

  // L0 内部
    ast -> types
    profile -> types
    ast -> dsl
    error -> validation

  // theme
    profile -> theme_select
    theme_data -> prepare
    theme_select -> prepare
    theme_data -> render

  // dsl / pipeline
    dsl -> pipeline
    ast -> pipeline

  // prepare / validation 读契约
    profile -> prepare
    profile -> validation
    layout -> validation
    dt_other -> prepare "mindmap_theme"

  // kinds 读 profile（单向！）
    profile -> dt_flowchart
    profile -> dt_arch
    layout -> dt_other "er/semantics 节点尺寸"
    dt_registry -> dt_flowchart
    dt_registry -> dt_arch
    dt_registry -> dt_other
    render -> dt_registry "scene_svg 分派"

  // 渲染链
    layout -> render
    graphic_style -> render
    prepare -> pipeline
    validation -> pipeline
    pipeline -> facade
    render -> facade

  // 反例环：registry → flowchart → spec → registry
    bad_registry -> bad_flowchart "若搬 ENTITY_TYPES 到这里"
    bad_flowchart --> bad_registry "render 调用 profile_for"
}
