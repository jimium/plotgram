import { IconPlus, IconTrash } from './Icons';
import {
  INTENT_PRESETS,
  EMPTY_INTENT_DRAFTS,
  hasIntentDrafts,
  type IntentDrafts,
  type TopologyKind,
  type GeometricKind,
  type PinAxis,
  type TopologyIntentDraft,
  type GeometricIntentDraft,
} from '../data/intentOptions';

interface IntentPanelProps {
  drafts: IntentDrafts;
  onChange: (drafts: IntentDrafts) => void;
  entityIds: string[];
}

const TOPOLOGY_KIND_OPTIONS: { value: TopologyKind; label: string }[] = [
  { value: 'below', label: 'below — A 在 B 下方' },
  { value: 'above', label: 'above — A 在 B 上方' },
];

const GEOMETRIC_KIND_OPTIONS: { value: GeometricKind; label: string }[] = [
  { value: 'pin', label: 'pin — 锁定节点坐标' },
  { value: 'align_vertical', label: 'align_vertical — x 中心对齐' },
  { value: 'align_horizontal', label: 'align_horizontal — y 中心对齐' },
];

const PIN_AXIS_OPTIONS: { value: PinAxis; label: string }[] = [
  { value: 'both', label: 'both' },
  { value: 'x', label: 'x' },
  { value: 'y', label: 'y' },
];

/** 解析逗号分隔的节点列表。 */
function parseNodeList(raw: string): string[] {
  return raw
    .split(/[,\s]+/)
    .map((s) => s.trim())
    .filter((s) => s.length > 0);
}

// ─── 单节点选择器 ─────────────────────────────────────────
interface NodeSelectProps {
  value: string;
  entityIds: string[];
  placeholder: string;
  onChange: (value: string) => void;
}

function NodeSelect({ value, entityIds, placeholder, onChange }: NodeSelectProps) {
  // 无实体时回退为文本输入
  if (entityIds.length === 0) {
    return (
      <input
        className="input input-sm"
        placeholder={placeholder}
        value={value}
        onChange={(e) => onChange(e.target.value)}
      />
    );
  }
  const inList = entityIds.includes(value);
  return (
    <select
      className="select select-sm"
      value={value}
      onChange={(e) => onChange(e.target.value)}
    >
      <option value="">{placeholder}</option>
      {entityIds.map((id) => (
        <option key={id} value={id}>{id}</option>
      ))}
      {/* 当前值不在列表中时额外显示，避免丢失 */}
      {!inList && value && <option value={value}>{value}</option>}
    </select>
  );
}

// ─── 多节点 chip 选择器 ──────────────────────────────────
interface NodeChipsProps {
  value: string;
  entityIds: string[];
  onChange: (value: string) => void;
}

function NodeChips({ value, entityIds, onChange }: NodeChipsProps) {
  const selected = parseNodeList(value);
  const available = entityIds.filter((id) => !selected.includes(id));

  const addNode = (id: string) => {
    if (!id || selected.includes(id)) return;
    onChange([...selected, id].join(', '));
  };

  const removeNode = (id: string) => {
    onChange(selected.filter((n) => n !== id).join(', '));
  };

  return (
    <div className="node-chips">
      {selected.map((id) => (
        <span key={id} className="node-chip">
          {id}
          <button
            type="button"
            className="node-chip-remove"
            onClick={() => removeNode(id)}
            title="移除"
          >
            ×
          </button>
        </span>
      ))}
      {available.length > 0 ? (
        <select
          className="select select-sm node-chip-add"
          value=""
          onChange={(e) => {
            if (e.target.value) addNode(e.target.value);
            e.target.value = '';
          }}
        >
          <option value="">+ 添加节点</option>
          {available.map((id) => (
            <option key={id} value={id}>{id}</option>
          ))}
        </select>
      ) : selected.length === 0 ? (
        <span className="hint">无可用节点</span>
      ) : null}
    </div>
  );
}

// ─── 主面板 ──────────────────────────────────────────────
export function IntentPanel({ drafts, onChange, entityIds }: IntentPanelProps) {
  const update = (patch: Partial<IntentDrafts>) => {
    onChange({ ...drafts, ...patch });
  };

  // ─── 拓扑意图 ─────────────────────────────────────────
  const addTopology = () => {
    update({
      topology: [...drafts.topology, { kind: 'below', from: '', to: '' }],
    });
  };

  const updateTopology = (index: number, patch: Partial<TopologyIntentDraft>) => {
    update({
      topology: drafts.topology.map((t, i) => (i === index ? { ...t, ...patch } : t)),
    });
  };

  const removeTopology = (index: number) => {
    update({ topology: drafts.topology.filter((_, i) => i !== index) });
  };

  // ─── 几何意图 ─────────────────────────────────────────
  const addGeometric = () => {
    update({
      geometric: [...drafts.geometric, { kind: 'pin', node: '', axis: 'both', nodes: '' }],
    });
  };

  const updateGeometric = (index: number, patch: Partial<GeometricIntentDraft>) => {
    update({
      geometric: drafts.geometric.map((g, i) => (i === index ? { ...g, ...patch } : g)),
    });
  };

  const removeGeometric = (index: number) => {
    update({ geometric: drafts.geometric.filter((_, i) => i !== index) });
  };

  // ─── 预设 ─────────────────────────────────────────────
  const loadPreset = (presetId: string) => {
    const preset = INTENT_PRESETS.find((p) => p.id === presetId);
    if (!preset) return;
    onChange({ ...preset.drafts, topology: preset.drafts.topology.map((t) => ({ ...t })) });
  };

  const clearAll = () => {
    onChange({ ...EMPTY_INTENT_DRAFTS });
  };

  const intentActive = drafts.enabled && hasIntentDrafts(drafts);

  return (
    <div className="intent-panel">
      {/* 启用开关 */}
      <label className="toggle">
        <input
          type="checkbox"
          checked={drafts.enabled}
          onChange={(e) => update({ enabled: e.target.checked })}
        />
        <span>启用布局意图（Layout Intent）</span>
      </label>

      {!drafts.enabled ? (
        <p className="hint">
          布局意图允许在不修改 diagram 源码 <code>relations</code> 的前提下，向布局算法注入额外的拓扑/几何约束，
          并返回每条意图的满足度报告。勾选后可添加意图。
        </p>
      ) : (
        <>
          {/* 预设 */}
          <div className="intent-presets">
            <span className="field-label">快速预设</span>
            <select
              className="select"
              value=""
              onChange={(e) => {
                if (e.target.value) loadPreset(e.target.value);
                e.target.value = '';
              }}
            >
              <option value="">选择预设载入…</option>
              {INTENT_PRESETS.map((p) => (
                <option key={p.id} value={p.id}>{p.label}</option>
              ))}
            </select>
            {intentActive && (
              <button type="button" className="link-btn intent-clear-btn" onClick={clearAll}>
                清空
              </button>
            )}
          </div>

          {/* 拓扑意图 */}
          <div className="intent-group">
            <div className="intent-group-head">
              <span className="intent-group-title">拓扑意图</span>
              <span className="hint">影响分层 rank 排序</span>
            </div>
            {drafts.topology.length === 0 && (
              <div className="intent-empty">无拓扑意图</div>
            )}
            {drafts.topology.map((t, i) => (
              <div key={i} className="intent-row">
                <select
                  className="select select-sm intent-kind-select"
                  value={t.kind}
                  onChange={(e) => updateTopology(i, { kind: e.target.value as TopologyKind })}
                >
                  {TOPOLOGY_KIND_OPTIONS.map((o) => (
                    <option key={o.value} value={o.value}>{o.label}</option>
                  ))}
                </select>
                <NodeSelect
                  value={t.from}
                  entityIds={entityIds}
                  placeholder="from"
                  onChange={(v) => updateTopology(i, { from: v })}
                />
                <span className="intent-arrow">→</span>
                <NodeSelect
                  value={t.to}
                  entityIds={entityIds}
                  placeholder="to"
                  onChange={(v) => updateTopology(i, { to: v })}
                />
                <button
                  type="button"
                  className="icon-btn intent-remove-btn"
                  onClick={() => removeTopology(i)}
                  title="删除"
                >
                  <IconTrash size={13} />
                </button>
              </div>
            ))}
            <button type="button" className="btn btn-ghost btn-sm intent-add-btn" onClick={addTopology}>
              <IconPlus size={13} />
              添加拓扑意图
            </button>
          </div>

          {/* 几何意图 */}
          <div className="intent-group">
            <div className="intent-group-head">
              <span className="intent-group-title">几何意图</span>
              <span className="hint">布局后修正坐标</span>
            </div>
            {drafts.geometric.length === 0 && (
              <div className="intent-empty">无几何意图</div>
            )}
            {drafts.geometric.map((g, i) => (
              <div key={i} className="intent-row intent-row-geo">
                <select
                  className="select select-sm intent-kind-select"
                  value={g.kind}
                  onChange={(e) => updateGeometric(i, { kind: e.target.value as GeometricKind })}
                >
                  {GEOMETRIC_KIND_OPTIONS.map((o) => (
                    <option key={o.value} value={o.value}>{o.label}</option>
                  ))}
                </select>
                {g.kind === 'pin' ? (
                  <>
                    <NodeSelect
                      value={g.node}
                      entityIds={entityIds}
                      placeholder="node"
                      onChange={(v) => updateGeometric(i, { node: v })}
                    />
                    <select
                      className="select select-sm"
                      value={g.axis}
                      onChange={(e) => updateGeometric(i, { axis: e.target.value as PinAxis })}
                    >
                      {PIN_AXIS_OPTIONS.map((o) => (
                        <option key={o.value} value={o.value}>{o.label}</option>
                      ))}
                    </select>
                  </>
                ) : (
                  <NodeChips
                    value={g.nodes}
                    entityIds={entityIds}
                    onChange={(v) => updateGeometric(i, { nodes: v })}
                  />
                )}
                <button
                  type="button"
                  className="icon-btn intent-remove-btn"
                  onClick={() => removeGeometric(i)}
                  title="删除"
                >
                  <IconTrash size={13} />
                </button>
              </div>
            ))}
            <button type="button" className="btn btn-ghost btn-sm intent-add-btn" onClick={addGeometric}>
              <IconPlus size={13} />
              添加几何意图
            </button>
          </div>

          <p className="hint">
            意图作为独立参数传入布局算法，不修改 diagram 源码。渲染后可在底部「意图报告」页签查看每条意图的满足状态。
          </p>
        </>
      )}
    </div>
  );
}
