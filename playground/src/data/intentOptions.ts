/**
 * Layout Intent 类型与辅助函数。
 *
 * 与 Rust 端 `drawify-core::layout::intent` 对齐：
 * - `TopologyIntent`：拓扑意图（below / above），影响 Sugiyama 分层 rank 排序。
 * - `GeometricIntent`：几何意图（pin / align_vertical / align_horizontal），布局后修正坐标。
 *
 * 详见 docs/architecture/layout-intent-usage.md。
 */

export type TopologyKind = 'below' | 'above';
export type GeometricKind = 'pin' | 'align_vertical' | 'align_horizontal';
export type PinAxis = 'x' | 'y' | 'both';

/** 拓扑意图（与 Rust `TopologyIntent` 的 serde 表示一致）。 */
export interface TopologyIntent {
  kind: TopologyKind;
  from: string;
  to: string;
}

/** 几何意图（与 Rust `GeometricIntent` 的 serde 表示一致）。 */
export type GeometricIntent =
  | { kind: 'pin'; node: string; axis: PinAxis }
  | { kind: 'align_vertical'; nodes: string[] }
  | { kind: 'align_horizontal'; nodes: string[] };

/** 意图叠加层（与 Rust `LayoutIntentOverlay` 一致）。 */
export interface LayoutIntentOverlay {
  topology: TopologyIntent[];
  geometric: GeometricIntent[];
}

/** 意图满足状态。 */
export type IntentStatus = 'satisfied' | 'partial' | 'conflicted' | 'not_found';

/** 单条意图执行结果。 */
export interface IntentResult {
  index: number;
  kind: string;
  status: IntentStatus;
  message: string | null;
}

/** 意图修正报告。 */
export interface RefinementReport {
  results: IntentResult[];
  satisfied: number;
  partial: number;
  conflicted: number;
  not_found: number;
}

/** 编辑态：可空字段，便于 UI 增删行。 */
export interface TopologyIntentDraft {
  kind: TopologyKind;
  from: string;
  to: string;
}

export interface GeometricIntentDraft {
  kind: GeometricKind;
  node: string;
  axis: PinAxis;
  nodes: string; // 逗号分隔，UI 友好
}

/** Intent 面板的编辑态。 */
export interface IntentDrafts {
  enabled: boolean;
  topology: TopologyIntentDraft[];
  geometric: GeometricIntentDraft[];
}

export const EMPTY_INTENT_DRAFTS: IntentDrafts = {
  enabled: false,
  topology: [],
  geometric: [],
};

/** 规范化 localStorage / 分享链接中的草稿。 */
export function normalizeIntentDrafts(raw: unknown): IntentDrafts {
  if (!raw || typeof raw !== 'object') {
    return { ...EMPTY_INTENT_DRAFTS };
  }
  const value = raw as Partial<IntentDrafts>;
  const topology: TopologyIntentDraft[] = Array.isArray(value.topology)
    ? value.topology
        .filter((t): t is TopologyIntentDraft => Boolean(t) && typeof t === 'object')
        .map((t) => ({
          kind: (t.kind === 'above' ? 'above' : 'below') as TopologyKind,
          from: typeof t.from === 'string' ? t.from : '',
          to: typeof t.to === 'string' ? t.to : '',
        }))
    : [];
  const geometric: GeometricIntentDraft[] = Array.isArray(value.geometric)
    ? value.geometric
        .filter((g): g is GeometricIntentDraft => Boolean(g) && typeof g === 'object')
        .map((g) => {
          const kind: GeometricKind =
            g.kind === 'pin' || g.kind === 'align_vertical' || g.kind === 'align_horizontal'
              ? g.kind
              : 'pin';
          return {
            kind,
            node: typeof g.node === 'string' ? g.node : '',
            axis: g.axis === 'x' || g.axis === 'y' || g.axis === 'both' ? g.axis : 'both',
            nodes: typeof g.nodes === 'string' ? g.nodes : '',
          };
        })
    : [];
  return {
    enabled: Boolean(value.enabled),
    topology,
    geometric,
  };
}

/** 解析逗号分隔的节点列表，去空白、去空项。 */
function parseNodeList(raw: string): string[] {
  return raw
    .split(/[,\s]+/)
    .map((s) => s.trim())
    .filter((s) => s.length > 0);
}

/** 将编辑态草稿转换为可提交给 WASM 的 overlay；返回 null 表示无有效意图。 */
export function buildIntentOverlay(drafts: IntentDrafts): LayoutIntentOverlay | null {
  if (!drafts.enabled) return null;

  const topology: TopologyIntent[] = drafts.topology
    .filter((d) => d.from.trim() && d.to.trim())
    .map((d) => ({ kind: d.kind, from: d.from.trim(), to: d.to.trim() }));

  const geometric: GeometricIntent[] = [];
  for (const d of drafts.geometric) {
    if (d.kind === 'pin') {
      const node = d.node.trim();
      if (node) geometric.push({ kind: 'pin', node, axis: d.axis });
    } else {
      const nodes = parseNodeList(d.nodes);
      if (nodes.length >= 2) {
        geometric.push({ kind: d.kind, nodes });
      }
    }
  }

  if (topology.length === 0 && geometric.length === 0) return null;
  return { topology, geometric };
}

/** 草稿是否有任何已填写的意图。 */
export function hasIntentDrafts(drafts: IntentDrafts): boolean {
  const topologyFilled = drafts.topology.some((d) => d.from.trim() && d.to.trim());
  const geometricFilled = drafts.geometric.some((d) =>
    d.kind === 'pin' ? d.node.trim() : parseNodeList(d.nodes).length >= 2,
  );
  return topologyFilled || geometricFilled;
}

// ─── 预设 ───────────────────────────────────────────────

export interface IntentPreset {
  id: string;
  label: string;
  description: string;
  drafts: IntentDrafts;
}

/**
 * 意图预设，对应 docs/architecture/layout-intent-usage.md 中的示例场景。
 * 节点 id 使用通用占位（a / b / c / d），用户加载后按当前 diagram 修改。
 */
export const INTENT_PRESETS: IntentPreset[] = [
  {
    id: 'below',
    label: 'Below：强制上下顺序',
    description: 'A 必须在 B 下方（注入约束边 B→A，使 rank(A) > rank(B)）',
    drafts: {
      enabled: true,
      topology: [{ kind: 'below', from: 'a', to: 'b' }],
      geometric: [],
    },
  },
  {
    id: 'above',
    label: 'Above：强制上下顺序',
    description: 'A 必须在 B 上方（注入约束边 A→B，使 rank(A) < rank(B)）',
    drafts: {
      enabled: true,
      topology: [{ kind: 'above', from: 'a', to: 'b' }],
      geometric: [],
    },
  },
  {
    id: 'align-vertical',
    label: 'AlignVertical：垂直对齐',
    description: 'A、B、C 三个节点的 x 中心对齐',
    drafts: {
      enabled: true,
      topology: [],
      geometric: [{ kind: 'align_vertical', node: '', axis: 'both', nodes: 'a, b, c' }],
    },
  },
  {
    id: 'align-horizontal',
    label: 'AlignHorizontal：水平对齐',
    description: 'D、E 两个节点的 y 中心对齐',
    drafts: {
      enabled: true,
      topology: [],
      geometric: [{ kind: 'align_horizontal', node: '', axis: 'both', nodes: 'd, e' }],
    },
  },
  {
    id: 'pin',
    label: 'Pin：锁定节点位置',
    description: '节点 A 在 grid snap 时不被移动（x、y 都锁定）',
    drafts: {
      enabled: true,
      topology: [],
      geometric: [{ kind: 'pin', node: 'a', axis: 'both', nodes: '' }],
    },
  },
  {
    id: 'mixed',
    label: '混合意图',
    description: 'Below(A,B) + Above(C,D) + Pin(A,x) + AlignVertical(B,C,D)',
    drafts: {
      enabled: true,
      topology: [
        { kind: 'below', from: 'a', to: 'b' },
        { kind: 'above', from: 'c', to: 'd' },
      ],
      geometric: [
        { kind: 'pin', node: 'a', axis: 'x', nodes: '' },
        { kind: 'align_vertical', node: '', axis: 'both', nodes: 'b, c, d' },
      ],
    },
  },
  {
    id: 'conflict',
    label: '冲突检测：成环',
    description: '真实边 A→B，意图 Below(A,B) 注入 B→A → 环 A→B→A → Conflicted',
    drafts: {
      enabled: true,
      topology: [{ kind: 'below', from: 'a', to: 'b' }],
      geometric: [],
    },
  },
];

// ─── 报告辅助 ───────────────────────────────────────────

export const STATUS_LABELS: Record<IntentStatus, string> = {
  satisfied: '已满足',
  partial: '部分满足',
  conflicted: '冲突',
  not_found: '节点不存在',
};

export const STATUS_COLORS: Record<IntentStatus, string> = {
  satisfied: 'success',
  partial: 'warning',
  conflicted: 'error',
  not_found: 'muted',
};

/** 把意图草稿渲染为可读的简短描述（用于报告行展示）。 */
export function describeTopologyIntent(t: TopologyIntentDraft): string {
  return `${t.kind}(${t.from || '?'}, ${t.to || '?'})`;
}

export function describeGeometricIntent(g: GeometricIntentDraft): string {
  if (g.kind === 'pin') {
    return `pin(${g.node || '?'}, ${g.axis})`;
  }
  return `${g.kind}(${g.nodes || '?'})`;
}
