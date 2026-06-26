import { useState, useCallback } from 'react';
import type {
  DiagramJson,
  DiagramAttributeJson,
  EntityJson,
  RelationJson,
  GroupJson,
  StyleDeclJson,
} from '../lib/wasm';

interface AstViewerProps {
  diagram: DiagramJson;
}

// 格式化属性值显示
function formatValue(value: DiagramAttributeJson['value']): string {
  if (value === null) return 'null';
  if (typeof value === 'object' && '$enum' in value) return `@${value.$enum}`;
  return String(value);
}

// 节点类型标签颜色
function typeColor(type: string): string {
  switch (type) {
    case 'flowchart': return 'var(--accent)';
    case 'sequence': return 'var(--accent-cyan)';
    case 'architecture': return '#a78bfa';
    case 'state': return 'var(--warning)';
    case 'er': return 'var(--success)';
    case 'mindmap': return '#f472b6';
    default: return 'var(--text-muted)';
  }
}

// 可折叠树节点
interface TreeNodeProps {
  label: string;
  badge?: string;
  badgeColor?: string;
  defaultOpen?: boolean;
  children: React.ReactNode;
}

function TreeNode({ label, badge, badgeColor, defaultOpen = true, children }: TreeNodeProps) {
  const [open, setOpen] = useState(defaultOpen);

  return (
    <div className="ast-node">
      <div className="ast-node-header" onClick={() => setOpen(o => !o)}>
        <span className={`ast-chevron ${open ? 'open' : ''}`}>
          <svg width="10" height="10" viewBox="0 0 10 10" fill="currentColor">
            <path d="M3 2l4 3-4 3V2z"/>
          </svg>
        </span>
        <span className="ast-label">{label}</span>
        {badge && (
          <span className="ast-badge" style={badgeColor ? { color: badgeColor } : undefined}>
            {badge}
          </span>
        )}
      </div>
      {open && <div className="ast-children">{children}</div>}
    </div>
  );
}

// 简单键值行
interface KeyLineProps {
  label: string;
  value: React.ReactNode;
  valueColor?: string;
}

function KeyLine({ label, value, valueColor }: KeyLineProps) {
  return (
    <div className="ast-key-line">
      <span className="ast-key">{label}</span>
      <span className="ast-value" style={valueColor ? { color: valueColor } : undefined}>
        {value}
      </span>
    </div>
  );
}

export function AstViewer({ diagram }: AstViewerProps) {
  const [copied, setCopied] = useState(false);

  const handleCopyJson = useCallback(() => {
    navigator.clipboard.writeText(JSON.stringify(diagram, null, 2));
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  }, [diagram]);

  return (
    <div className="ast-viewer-root">
      <div className="ast-toolbar">
        <span className="ast-toolbar-info">
          {diagram.entities.length} 实体 · {diagram.relations.length} 边 · {diagram.groups.length} 组
        </span>
        <button type="button" className="btn btn-ghost btn-sm" onClick={handleCopyJson}>
          {copied ? '已复制!' : '复制 JSON'}
        </button>
      </div>

      <div className="ast-tree">
        {/* 根节点：Diagram */}
        <TreeNode
          label="diagram"
          badge={diagram.diagram_type}
          badgeColor={typeColor(diagram.diagram_type)}
          defaultOpen={true}
        >
          {/* 属性 */}
          {diagram.attributes.length > 0 && (
            <TreeNode label="attributes" badge={`${diagram.attributes.length}`} defaultOpen={false}>
              {diagram.attributes.map((attr, i) => (
                <KeyLine
                  key={i}
                  label={attr.key}
                  value={formatValue(attr.value)}
                  valueColor="var(--accent)"
                />
              ))}
            </TreeNode>
          )}

          {/* 实体 */}
          {diagram.entities.length > 0 && (
            <TreeNode label="entities" badge={`${diagram.entities.length}`} defaultOpen={true}>
              {diagram.entities.map((entity, i) => (
                <EntityNode key={i} entity={entity} index={i} />
              ))}
            </TreeNode>
          )}

          {/* 关系 */}
          {diagram.relations.length > 0 && (
            <TreeNode label="relations" badge={`${diagram.relations.length}`} defaultOpen={false}>
              {diagram.relations.map((rel, i) => (
                <RelationNode key={i} relation={rel} index={i} />
              ))}
            </TreeNode>
          )}

          {/* 分组 */}
          {diagram.groups.length > 0 && (
            <TreeNode label="groups" badge={`${diagram.groups.length}`} defaultOpen={false}>
              {diagram.groups.map((group, i) => (
                <GroupNode key={i} group={group} index={i} />
              ))}
            </TreeNode>
          )}

          {/* 样式声明 */}
          {diagram.style_decls.length > 0 && (
            <TreeNode label="style_decls" badge={`${diagram.style_decls.length}`} defaultOpen={false}>
              {diagram.style_decls.map((decl, i) => (
                <StyleDeclNode key={i} decl={decl} index={i} />
              ))}
            </TreeNode>
          )}
        </TreeNode>
      </div>
    </div>
  );
}

function EntityNode({ entity, index }: { entity: EntityJson; index: number }) {
  return (
    <TreeNode
      label={`entity`}
      badge={entity.id}
      badgeColor="var(--accent)"
      defaultOpen={index === 0}
    >
      <KeyLine label="id" value={entity.id} valueColor="var(--accent)" />
      <KeyLine label="label" value={entity.label} valueColor="var(--text)" />
      {entity.group_id && (
        <KeyLine label="group" value={entity.group_id} valueColor="var(--text-muted)" />
      )}
      <KeyLine
        label="span"
        value={`L${entity.span.start.line}:${entity.span.start.column}–L${entity.span.end.line}:${entity.span.end.column}`}
        valueColor="var(--text-faint)"
      />
    </TreeNode>
  );
}

function RelationNode({ relation, index }: { relation: RelationJson; index: number }) {
  return (
    <TreeNode
      label={`relation`}
      badge={`${relation.from} → ${relation.to}`}
      badgeColor="var(--success)"
      defaultOpen={index === 0}
    >
      <KeyLine label="from" value={relation.from} valueColor="var(--accent)" />
      <KeyLine label="to" value={relation.to} valueColor="var(--accent)" />
      <KeyLine label="arrow" value={relation.arrow} valueColor="var(--text-muted)" />
      {relation.label && (
        <KeyLine label="label" value={relation.label} valueColor="var(--warning)" />
      )}
      <KeyLine
        label="span"
        value={`L${relation.span.start.line}:${relation.span.start.column}–L${relation.span.end.line}:${relation.span.end.column}`}
        valueColor="var(--text-faint)"
      />
    </TreeNode>
  );
}

function GroupNode({ group, index }: { group: GroupJson; index: number }) {
  return (
    <TreeNode
      label={`group`}
      badge={group.label}
      badgeColor="#a78bfa"
      defaultOpen={index === 0}
    >
      <KeyLine label="id" value={group.id} valueColor="var(--accent)" />
      <KeyLine label="label" value={group.label} valueColor="var(--text)" />
      {group.parent_id && (
        <KeyLine label="parent" value={group.parent_id} valueColor="var(--text-muted)" />
      )}
      <KeyLine
        label="entities"
        value={group.entity_ids.join(', ') || '—'}
        valueColor="var(--text-muted)"
      />
      <KeyLine
        label="children"
        value={group.child_group_ids.join(', ') || '—'}
        valueColor="var(--text-muted)"
      />
    </TreeNode>
  );
}

function StyleDeclNode({ decl, index }: { decl: StyleDeclJson; index: number }) {
  return (
    <TreeNode
      label={`${decl.kind}_style`}
      badge={decl.target}
      badgeColor="var(--warning)"
      defaultOpen={index === 0}
    >
      <KeyLine label="kind" value={decl.kind} valueColor="var(--text-muted)" />
      <KeyLine label="target" value={decl.target} valueColor="var(--warning)" />
    </TreeNode>
  );
}
