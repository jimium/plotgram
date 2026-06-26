# Graphic Styles

各风格的视觉特征与技术实现说明。输入仅接受规范名（与 `GraphicStyleId::as_str()` 一致），不支持别名。

| 风格 | 简述 | 核心实现 |
|---|---|---|
| **standard** | 标准纯色 | 无自定义渲染，纯 SVG 默认样式，作为兜底 |
| **excalidraw** | 手绘点描 | `clipped_dot_fill` 将圆点限制在形状 path 内 + 双层 `rough_polyline` 抖动描边 |
| **cross-hatch** | 略密点描 | 复用 excalidraw 抖动算法，点距更密、透明度略高 |
| **blueprint** | 工程蓝图 | 细线 + 半透明填充 + miter 尖角；`blueprint_center_cross` 中心线标记 |
| **spatial-clarity** | 现代UI风 | 双层 `feDropShadow` + 圆角 + 低透明度描边；edge 使用 `smooth_polyline_path` |
| **neon-glow** | 霓虹光晕 | 双层 `feGaussianBlur` glow + 细亮描边 + 低不透明度填充 |
| **stipple** | 圆点填充 | `dot_fill` 错位圆点 + 低粗糙度 `rough_polyline` 轮廓 |

## 架构说明

- 每个风格实现 `GraphicStylePainter` trait，注册在 `mod.rs` 的 `painter_for()` 中。
- `excalidraw` 和 `cross-hatch` 共享同一结构体 `ExcalidrawGraphicStylePainter`，通过 `FillMode` 枚举区分点描密度。
- 通用算法（`rough_polyline`、`clipped_dot_fill`、`dot_fill`、`adaptive_gap`、`polyline_path`、`smooth_polyline_path` 等）在 `common.rs` 中。
