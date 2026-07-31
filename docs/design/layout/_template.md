# 布局内核文档模板

新建 `docs/design/layout/<kernel>/` 时按此骨架填充。封面宜短；细节拆子文，勿把实现债写进设计档。

## 文件夹约定

```text
<kernel>/
  README.md          # 必读封面（签名 · 基本逻辑摘要 · 写权 · 边几何）
  architecture.md    # 可选：目标架构（相序、算法选型、IR、参数、里程碑）
                     #   复杂核（如 Hier）建议有；简单核可把相序/写权并进 README
  scope.md           # 能力范围 · 非目标 · 典型域（可并入 README）
  phases/            # 可选：按相展开的设计细节（从 architecture 下沉）
  vs-reference.md    # 可选：与 yFiles / Graphviz 等对照
```

**不要**单独维护与 `architecture.md` / README 重复的 `pipeline.md`（易双真源）。

## README 封面四件事

1. **签名**：一句话（流形 / 目标函数直觉）。  
2. **基本逻辑**：相序或骨架摘要（有 `architecture.md` 则链过去，勿再抄长文）。  
3. **能力范围 + 非目标**：写清故意不做的。  
4. **典型域**：默认挂哪些 profile；引擎注册名。

另附：

- **写权表**：本核负责的自由度 → 哪一相写（可与 architecture 同表，README 留短表即可）。  
- **边几何**：内建 Ink / `DeferToRouter` / 二者皆可。  
- **证据与纪律**：链到 [`write-authority.md`](write-authority.md) 与 `docs/reference/yfiles/…`，不复制长文。

## `architecture.md`（复杂核）

适合 Hier 这类多相位核，建议覆盖：

- 硬约束与总体管线（Compose / Metric / Ink 或等价切分）  
- 核心 IR（Plan / Metric / DemandBoard…）  
- **算法选型表**（主选 / 替代 / 后置）  
- 参数与 preset（算法级；与 `profile:` 区分）  
- 落地里程碑（设计口径，非进度日记）  
- 反模式速查  

相级长文再拆进 `phases/`，避免 architecture 无限膨胀。

## 写作约束

- 汉语行文；算法术语保留英文（layering、crossing minimization…）。  
- 伪代码 Rust 风，不承诺可编译；标注**写的自由度**。  
- 不按图种开平行管线；profile / Scheme 差异用参数表表达。  
- 「工程坑」只记仍有效的契约风险；进度日记不进本夹。  
- 同一事实只留一个真源；摘要页只链、不复制。
