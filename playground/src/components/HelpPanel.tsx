import { useEffect } from 'react';
import { IconClose } from './Icons';

interface HelpPanelProps {
  open: boolean;
  onClose: () => void;
}

const SNIPPET = `diagram architecture {
    title: "服务架构"

    group frontend "前端层" {
        entity[frontend] web "Web"
        entity[frontend] app "Mobile"
    }

    group backend "后端层" {
        entity[service] api "API"
        entity[service] worker "Worker"
        entity[database] db "DB"

        // 组内边：两端都在本 group 内
        api -> worker "dispatch"
        api -> db
        worker -> db
    }

    // 跨组边：写在顶层
    web -> api
    app -> api
}`;

interface RowProps {
  name: string;
  desc: string;
}

function Row({ name, desc }: RowProps) {
  return (
    <tr>
      <td><code>{name}</code></td>
      <td>{desc}</td>
    </tr>
  );
}

export function HelpPanel({ open, onClose }: HelpPanelProps) {
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [open, onClose]);

  if (!open) return null;

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal help-modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-head">
          <h2>Drawify 语法速查</h2>
          <button type="button" className="icon-btn" onClick={onClose} aria-label="关闭">
            <IconClose />
          </button>
        </div>

        <div className="modal-body help-body">
          <section>
            <h3>结构</h3>
            <table className="help-table">
              <tbody>
                <Row name="diagram <type> { … }" desc="声明图表，type 为 flowchart / sequence / architecture / state / er / mindmap" />
                <Row name='entity[&lt;type&gt;] <id> "标签" { … }' desc="声明实体，语义类型写在方括号中，花括号内写其他属性" />
                <Row name='group <id> "标签" { … }' desc="分组；组内可写 entity、嵌套 group、属性，以及组内 edge（两端必须属于本 group 后代）" />
              </tbody>
            </table>
          </section>

          <section>
            <h3>关系（箭头）</h3>
            <table className="help-table">
              <tbody>
                <Row name="a -> b" desc="实线有向边" />
                <Row name="a --> b" desc="虚线 / 返回边" />
                <Row name="a <-> b" desc="双向关系" />
                <Row name='a -> b "标签"' desc="带标签的边" />
              </tbody>
            </table>
          </section>

          <section>
            <h3>常用属性</h3>
            <table className="help-table">
              <tbody>
                <Row name="title: &quot;…&quot;" desc="图表标题" />
                <Row name="direction: top-to-bottom | left-to-right" desc="布局方向" />
                <Row name="layout: flowchart | er | sugiyama-v2 | …" desc="布局算法" />
                <Row name="edge_routing: orthogonal | spline | bezier | …" desc="边路由方式" />
                <Row name="snap: true | false" desc="网格吸附（默认 true，flowchart / er / sugiyama-v2 / architecture-v2）" />
                <Row name="theme: common.clean-light | common.dracula | mindmap.vivid-branches | …" desc="颜色/字体主题（StyleSheet ID）" />
                <Row name="render_style: standard | excalidraw | …" desc="笔触皮肤（与 theme 正交）" />
                <Row name="entity[&lt;type&gt;]" desc="实体语义类型写在方括号中（如 start / process / decision / service 等），决定形状与图标" />
              </tbody>
            </table>
          </section>

          <section>
            <h3>最小示例</h3>
            <pre className="help-snippet">{SNIPPET}</pre>
          </section>

          <p className="hint">提示：在编辑器中输入时按 <kbd>Ctrl</kbd>/<kbd>⌘</kbd> + <kbd>Space</kbd> 可触发补全。</p>
        </div>
      </div>
    </div>
  );
}
