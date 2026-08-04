import { StateField, StateEffect, type EditorState } from '@codemirror/state';
import { type CompletionContext, type CompletionResult, type Completion } from '@codemirror/autocomplete';

// ─── AST 缓存：从 WASM parse 结果中提取的上下文信息 ────────────

export interface EntityInfo {
  id: string;
  label: string;
  groupId: string | null;
}

export interface GroupInfo {
  id: string;
  label: string;
  parentId: string | null;
}

export interface DiagramContext {
  entities: EntityInfo[];
  groups: GroupInfo[];
}

export const emptyContext: DiagramContext = { entities: [], groups: [] };

/** StateEffect：用于从外部更新 diagramContextField */
export const setDiagramContext = StateEffect.define<DiagramContext>();

/**
 * StateField：缓存最近一次成功解析的 diagram 上下文。
 * 由 CodeEditor 在 WASM parse 成功后通过 setDiagramContext effect 更新。
 */
export const diagramContextField = StateField.define<DiagramContext>({
  create: () => emptyContext,
  update(value, tr) {
    for (const e of tr.effects) {
      if (e.is(setDiagramContext)) {
        return e.value;
      }
    }
    return value;
  },
});

// ─── 辅助：从 editor state 读取上下文 ──────────────────────

function getContext(state: EditorState): DiagramContext {
  return state.field(diagramContextField, false) ?? emptyContext;
}

/**
 * 根据光标行号判断当前所在的 group。
 * 通过扫描文本中 group 块的花括号嵌套来确定。
 */
function findCurrentGroup(state: EditorState, pos: number): string | null {
  const doc = state.doc;
  const lines: { text: string; from: number }[] = [];
  for (let i = 1; i <= doc.lines; i++) {
    const line = doc.line(i);
    lines.push({ text: line.text, from: line.from });
  }

  // 用栈跟踪 group 嵌套
  const stack: { id: string; depth: number }[] = [];
  let depth = 0;

  for (const line of lines) {
    // 检测 group 声明：`group <id> "label" {`
    const groupMatch = line.text.match(/\bgroup\s+([A-Za-z_]\w*)\s+"/);
    if (groupMatch) {
      const groupId = groupMatch[1];
      // 检查这一行是否有 `{`
      if (line.text.includes('{')) {
        depth++;
        stack.push({ id: groupId, depth });
      }
    }

    // 如果光标在当前行，返回栈顶 group
    if (pos >= line.from && pos <= line.from + line.text.length) {
      return stack.length > 0 ? stack[stack.length - 1].id : null;
    }

    // 处理右括号减少深度
    const closeCount = (line.text.match(/\}/g) || []).length;
    for (let c = 0; c < closeCount; c++) {
      if (stack.length > 0) {
        stack.pop();
        depth--;
      }
    }
  }

  return null;
}

// ─── 补全逻辑 ─────────────────────────────────────────────

function entityCompletions(
  entities: EntityInfo[],
  currentGroup: string | null,
): Completion[] {
  // 排序：同组 entity 优先，按 label 展示
  const sorted = [...entities].sort((a, b) => {
    const aInGroup = a.groupId === currentGroup;
    const bInGroup = b.groupId === currentGroup;
    if (aInGroup && !bInGroup) return -1;
    if (!aInGroup && bInGroup) return 1;
    return a.id.localeCompare(b.id);
  });

  return sorted.map((e) => ({
    label: e.id,
    type: 'variable',
    detail: e.groupId === currentGroup ? `${e.label} (本组)` : e.label,
  }));
}

/**
 * 上下文感知补全源。
 * 处理三种场景：
 * 1. 箭头后 → 补全 entity ID
 * 2. 属性值 → 补全枚举值（委托给原有逻辑）
 * 3. 普通单词 → 补全关键字（委托给原有逻辑）
 */
export function contextAwareCompletions(context: CompletionContext): CompletionResult | null {
  const state = context.state;
  const pos = context.pos;
  const line = state.doc.lineAt(pos);
  const textBefore = line.text.slice(0, pos - line.from);

  // ── 场景 1：箭头后补全 entity ID ──
  // 匹配模式：`<word> (->|-->|<->) <partial>`
  const arrowMatch = textBefore.match(/(\w+)\s*(?:->|-->)\s*(\w*)$/);
  if (arrowMatch) {
    const partial = arrowMatch[2];
    const from = pos - partial.length;
    const ctx = getContext(state);
    if (ctx.entities.length === 0) return null;

    const currentGroup = findCurrentGroup(state, pos);
    const options = entityCompletions(ctx.entities, currentGroup);
    return { from, options, validFor: /^\w*$/ };
  }

  // ── 场景 1b：edge 的起点（行首 ident）也补全 entity ID ──
  // 匹配模式：行首（可能有空格）后跟 partial word，且不是属性赋值
  const lineStartMatch = textBefore.match(/^\s*(\w*)$/);
  if (lineStartMatch && lineStartMatch[1].length > 0) {
    const partial = lineStartMatch[1];
    // 排除关键字开头的情况
    const KEYWORDS = new Set(['diagram', 'entity', 'group', 'config', 'node_style', 'edge_style', 'meta']);
    if (!KEYWORDS.has(partial) && !partial.includes(':')) {
      const from = pos - partial.length;
      const ctx = getContext(state);
      if (ctx.entities.length > 0) {
        const currentGroup = findCurrentGroup(state, pos);
        const options = entityCompletions(ctx.entities, currentGroup);
        // 也加入关键字补全
        return { from, options, validFor: /^\w*$/ };
      }
    }
  }

  return null;
}
