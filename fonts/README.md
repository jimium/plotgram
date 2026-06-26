# Fonts Directory

Drawify 渲染图表所用的字体文件，主要用于解决 resvg 渲染 PNG/WebP 时中文显示问题。

## 加载方式

字体**不会**编译进二进制程序，运行时从文件系统加载。优先级（高 → 低）：

1. **CLI 参数**：`drawify render --fonts-dir /path/to/fonts ...`
2. **环境变量**：`DRAWIFY_FONTS_DIR=/path/to/fonts`
3. **默认路径**：当前工作目录（cwd）下的 `fonts/` 文件夹

请将本目录中的字体文件部署到运行环境的 `fonts/` 目录，或通过环境变量 / `--fonts-dir` 指向实际位置。

## 字体文件

| 文件名 | 字重 | 用途 |
|--------|------|------|
| NotoSansCJKsc-Regular.otf | Regular (400) | 图表中普通文本标签 |
| NotoSansCJKsc-Bold.otf | Bold (700) | 图表标题、强调文本 |

## 为什么选 Noto Sans CJK SC

- **无衬线字体**：与 SVG 中已有的 Segoe UI / Helvetica 风格一致，图表小字号下清晰易读
- **CJK 支持**：完整覆盖简体中文、日文、韩文字符集
- **只需两个字重**：图表场景用 Regular + Bold 即可，避免打包体积过大（单个 OTF 约 16MB）

## 字体名称解析

```
Noto - Sans - CJK - sc
  |      |      |    |
  |      |      |    └─ sc = Simplified Chinese (简体中文)
  |      |      └─ CJK = Chinese, Japanese, Korean (中日韩)
  |      └─ Sans = 无衬线字体 (如黑体)
  └─ Noto = Google 开源字体项目名称
```

## OTF 格式说明

OTF（OpenType Font）是跨平台字体格式：

- 支持 TrueType 和 PostScript 两种字形描述方式
- 支持高级排版特性（连字、花体字等）
- 支持 CJK 多语言字符集
- 跨平台兼容（Windows、macOS、Linux）

## 许可证

字体采用 **SIL Open Font License 1.1** 许可证，允许免费使用、修改、再分发和嵌入软件。

## 目录结构

```
fonts/
├── README.md                      # 本文件
├── NotoSansCJKsc-Regular.otf      # 常规体
└── NotoSansCJKsc-Bold.otf         # 粗体
```
