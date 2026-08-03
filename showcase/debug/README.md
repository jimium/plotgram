# Layout Debug Inspector（debug-inspector.md T2）

hierarchical 布局的调试检视页：wasm 桥在浏览器里实时跑完整
`parse → measure → layout` 管线，产出 `LayoutDebugTrace`（与 CLI
`debug-layout` 同源），画布叠加 ranks/dummies/reversed/ports 等决策层。

## 构建

```bash
./scripts/build-inspector.sh        # wasm-pack build → showcase/debug/pkg/
```

产物 `pkg/` 不入库（本目录 .gitignore）。

## 运行

ES module + wasm 需要 HTTP 环境（file:// 不行）：

```bash
python3 -m http.server -d showcase/debug 8090
# 打开 http://localhost:8090
```

## 使用

- 左栏编辑 DSL 源码，250ms debounce 后 wasm 即时重跑；或从下拉选样例
- 也可把 CLI 产物 `*.trace.json` 拖入 dropzone 做纯查看
- 中栏：滚轮缩放、拖拽平移；层开关控制叠层；点选元素联动右栏字段
- 右栏：信封头（schema_version/layout/kind/orientation/metrics/notes）
  + 选中对象的全部 trace 字段
- URL hash 保存层开关与选中 id，可分享：`#layers=product,ranks&sel=node:a&layout=hierarchical`

## 边界

- 只读投影：页面不回写任何几何（写权纪律）
- 未实现的 `channels` 恒为 null，不注册层
- 未知 `extension.kind` 降级为 common 层 + raw JSON 面板
