# Tautcore Brand

正式品牌资产。方向定稿：**S2a22（文本环绕节点）+ S5（taut / core 双色字标）**。

## 文件

| 文件 | 用途 |
|------|------|
| `logo.svg` | **主品牌**浅底彩色 lockup（圆角底 mark + 双色字标） |
| `logo-dark.svg` | **主品牌**深底彩色 lockup |
| `logo-icon.svg` | 标准方标（圆角底，通用 / ≥48px） |
| `logo-icon-16.svg` | **16px** 专用简化方标（favicon） |
| `logo-icon-32.svg` | **32px** 光学优化方标（侧栏 / 导航） |
| `logo-icon-64.svg` | **64px** 光学优化方标（App icon / 大图） |
| `logo-mono.svg` | **全黑** lockup（透明底，印刷 / 法务 / 水印） |
| `logo-mono-white.svg` | **全白** lockup（透明底，深色海报 / 视频） |
| `logo-mark.svg` | **无圆角底** mark（黑线+圆，透明底，文档 / CLI） |
| `logo-mark-white.svg` | 无圆角底 mark（白） |
| `logo-mark-current.svg` | 无圆角底 mark（`currentColor`，随 CSS `color`） |
| `fonts/` | Space Grotesk Bold 源字体（OFL，再生 path 用） |
| `options/` | 探索稿，非正式资产 |

## 概念

**文本 → 图。**

- 三根横线 = 书写 / DSL / 文本行
- **黄金比**缩短 + 右下节点圆 = 文本环绕并收束成节点
- 底行与圆**底部对齐**，形成稳定的「环绕」负形
- 字标拆词：`taut` + `core`（仅彩色主品牌拆色）

## 视觉原则

- **纯扁平**：实色，无渐变 / 光晕 / 阴影 / 滤镜
- **双色只作主品牌色**：产品顶栏、官网、App 用 `logo.svg` / `logo-dark.svg`
- **单色覆盖其余场景**：印刷、水印、法务、单色 UI → `logo-mono*`
- **符号版给开发者语境**：文档、CLI、README 行内 → `logo-mark*`（无圆角底）
- **尺寸分级**：按显示像素选用对应 `logo-icon-*`，勿把 16px 稿放大到 64

## 色板

| Token | Hex | 用途 |
|-------|-----|------|
| `ink` | `#121212` | 墨色、全黑单色、浅底字标 `plot` |
| `paper` | `#FAF8F4` | 彩色深底字标 / 浅底线稿（仅彩色资产） |
| `teal` | `#0F9F8A` | 浅底 `gram`、深底 icon 节点（**仅彩色**） |
| `mint` | `#2DD4A8` | 深底 `gram`、浅底 icon 节点（**仅彩色**） |
| `white` | `#FFFFFF` | 全白单色 / 白 mark |

单色与 bare mark **不使用**青绿。

## Mark 几何

### 标准（`logo-icon.svg` / `logo-icon-64.svg` / bare 共用线圆）

- 行距等距：y = 14.4 / 26.4 / 38.4
- 线长：上 `28.8`，中 `13.5`（为环绕气口略短于 φ），底 `11.0`
- 节点：`(31.9, 33)` r=`6.9`——**右缘对齐上线右端**（38.8），底对齐底行
- 圆角底版：`rx = 12`
- Bare 版：无 `<rect>`，透明底（`logo-mark*`）

### 尺寸变体

| 文件 | 显示尺寸 | 线数 | 线宽 | 节点 | 说明 |
|------|----------|------|------|------|------|
| `logo-icon-16.svg` | 16×16 | 2（上+底） | 5 | `(29, 30.5)` r=`8` | 去掉中线；底线缩短避免与圆碰撞 |
| `logo-icon-32.svg` | 32×32 | 3 | 3.5 | `(31.6, 32.8)` r=`7.2` | 略加粗，保持环绕气口 |
| `logo-icon-64.svg` | 64×64 | 3 | 3 | 同标准 | 与 `logo-icon.svg` 几何一致 |
| `logo-icon.svg` | ≥48 通用 | 3 | 3 | 同标准 | 不确定尺寸时的默认方标 |

方案 1 约束（各尺寸共用）：圆右缘 = 上线右端；圆底 = 底线底缘。

## 字标（已 path 化）

所有 lockup 字标为 **Space Grotesk Bold outlined paths**，无 `<text>`。

- 彩色：`plot` + `gram` 双色；词间 gap ≈2px
- 单色：整词 `tautcore` 同色
- 源字体：`fonts/SpaceGrotesk-Bold.ttf`（OFL）

## 使用

```html
<!-- 主品牌：产品顶栏 / 官网 -->
<img src="/assets/brand/logo.svg" alt="Tautcore" />
<img src="/assets/brand/logo-dark.svg" alt="Tautcore" />

<!-- Favicon（务必用 16px 稿） -->
<link rel="icon" href="/assets/brand/logo-icon-16.svg" type="image/svg+xml" />

<!-- 方标：按显示像素选 -->
<img src="/assets/brand/logo-icon-16.svg" width="16" height="16" alt="Tautcore" />
<img src="/assets/brand/logo-icon-32.svg" width="32" height="32" alt="Tautcore" />
<img src="/assets/brand/logo-icon-64.svg" width="64" height="64" alt="Tautcore" />
<img src="/assets/brand/logo-icon.svg" width="48" height="48" alt="Tautcore" />

<!-- 全黑 / 全白（透明底） -->
<img src="/assets/brand/logo-mono.svg" alt="Tautcore" />
<img src="/assets/brand/logo-mono-white.svg" alt="Tautcore" />

<!-- 文档 / CLI：无圆角底 -->
<img src="/assets/brand/logo-mark.svg" width="20" height="20" alt="" />
```

| 场景 | 用哪个 | 备注 |
|------|--------|------|
| 产品顶栏、官网、App 横条 | `logo.svg` / `logo-dark.svg` | 双色 lockup |
| Favicon / 标签页图标 | `logo-icon-16.svg` | 仅 16px；勿放大 |
| 导航 / 侧栏方标 ~28–36px | `logo-icon-32.svg` | website、showcase |
| App icon / 大图 ~48–128px | `logo-icon-64.svg` 或 `logo-icon.svg` | 64 与标准几何相同 |
| 不确定尺寸的方标 | `logo-icon.svg` | 默认三线标准稿 |
| PDF、水印、法务页 | `logo-mono.svg` | 全黑透明底 |
| 深色海报 / 视频角标 | `logo-mono-white.svg` | 全白透明底 |
| README、文档、CLI 提示符旁 | `logo-mark.svg` 或 `logo-mark-current.svg` | 无圆角底 |

`logo-mark-current.svg` 适合内联 SVG，用 CSS `color` 控制颜色。

## 同步提醒

各应用**各自拷贝**所需文件，不要跨目录引用 `assets/brand/`。源目录更新后需重新复制：

| 目标 | 本地副本 |
|------|----------|
| `playground/public/` | `logo.svg`、`logo-dark.svg`、`favicon.svg`（← `logo-icon-16.svg`） |
| `showcase/assets/brand/` | `logo-icon-16.svg`、`logo-icon-32.svg` |
| `website/public/assets/brand/` | 全套 lockup / icon / mono / mark |

## 演进记录

1. 旧品牌：Drawquill / Drawify，紫青渐变 D 形标
2. 探索：`options/preview-tautcore-*.html`
3. 定稿：S2a22 + S5 双色
4. 精修：黄金比线长、字距、path 化
5. 体系：16/32/64 尺寸分级、全黑/全白单色、无圆角底 mark
6. 16px：缩短底线，消除与节点圆碰撞
