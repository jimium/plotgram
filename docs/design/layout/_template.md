# 布局内核文档模板

新建 `docs/design/layout/<kernel>/` 时按此骨架填充。封面宜短；细节拆子文，勿把实现债写进设计档。

## 文件夹约定

```text
<kernel>/
  README.md          # 必读封面
  pipeline.md        # 基本逻辑 / 相切分 / 写权表（Hier 类必有；简单核可并入 README）
  scope.md           # 能力范围 · 非目标 · 典型域（可并入 README）
  phases/            # 可选：按相展开的设计细节
  vs-reference.md    # 可选：与 yFiles / Graphviz 等对照
```

## README 封面四件事

1. **签名**：一句话（流形 / 目标函数直觉）。  
2. **基本逻辑**：相序或骨架（可链到 `pipeline.md`）。  
3. **能力范围 + 非目标**：写清故意不做的。  
4. **典型域**：默认挂哪些 profile；引擎注册名。

另附：

- **写权表**：本核负责的自由度 → 哪一相写。  
- **边几何**：内建 Ink / `DeferToRouter` / 二者皆可。  
- **证据与纪律**：链到 [`write-authority.md`](write-authority.md) 与 `docs/reference/yfiles/…`，不复制长文。

## 写作约束

- 汉语行文；算法术语保留英文（layering、crossing minimization…）。  
- 伪代码 Rust 风，不承诺可编译；标注**写的自由度**。  
- 不按图种开平行管线；profile / Scheme 差异用参数表表达。  
- 「工程坑」只记仍有效的契约风险；进度日记不进本夹。
