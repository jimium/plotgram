export type DiagramKind =
  | 'flowchart'
  | 'sequence'
  | 'architecture'
  | 'state'
  | 'er'
  | 'mindmap';

export const KIND_LABELS: Record<DiagramKind, string> = {
  flowchart: '流程图',
  sequence: '时序图',
  architecture: '架构图',
  state: '状态图',
  er: 'ER 图',
  mindmap: '思维导图',
};
