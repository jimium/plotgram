interface CodeBlockProps {
  code: string;
  language?: string;
  title?: string;
}

const KEYWORDS = new Set(['diagram', 'entity', 'layout', 'title', 'config', 'direction', 'type', 'semantic', 'status', 'owner', 'group', 'node_style', 'edge_style', 'style', 'meta', 'description', 'icon', 'line_style', 'cardinality', 'border_style', 'color', 'render_style', 'theme', 'edge_routing', 'group_frame', 'align', 'snap', 'layout', 'gap', 'axis', 'track', 'cross', 'border']);

const TYPES = new Set(['flowchart', 'sequence', 'architecture', 'state', 'er', 'mindmap', 'left-to-right', 'top-to-bottom', 'right-to-left', 'bottom-to-top', 'horizontal', 'vertical', 'radial', 'orthogonal', 'straight', 'bezier', 'spline', 'circular', 'organic', 'true', 'false', 'healthy', 'degraded', 'down', 'unknown', 'solid', 'dashed', 'dotted', 'standard', 'excalidraw', 'blueprint', 'none', 'fit', 'equal', 'uniform', 'start', 'center', 'end', 'stretch', 'shared', 'shared_lines', 'auto']);

const ENTITY_TYPES = new Set(['start', 'end', 'process', 'decision', 'database', 'service', 'gateway', 'browser', 'cache', 'user', 'server', 'client', 'api', 'auth', 'person', 'queue', 'storage', 'external', 'actor', 'participant', 'boundary', 'control', 'lifeline', 'frontend', 'backend', 'initial', 'final', 'choice', 'root', 'main', 'branch', 'leaf']);

type Token = { text: string; className?: string };

function tokenizeLine(line: string): Token[] {
  const tokens: Token[] = [];
  let i = 0;
  const len = line.length;

  while (i < len) {
    const ch = line[i];

    if (ch === '/' && i + 1 < len && line[i + 1] === '/') {
      tokens.push({ text: line.slice(i), className: 'code-comment' });
      break;
    }

    if (ch === '"') {
      let j = i + 1;
      while (j < len && line[j] !== '"') j++;
      if (j < len) j++;
      tokens.push({ text: line.slice(i, j), className: 'code-string' });
      i = j;
      continue;
    }

    if (ch === '<' && i + 2 < len && line[i + 1] === '-' && line[i + 2] === '>') {
      tokens.push({ text: '<->', className: 'code-arrow' });
      i += 3;
      continue;
    }

    if (ch === '-' && i + 2 < len && line[i + 1] === '-' && line[i + 2] === '>') {
      tokens.push({ text: '-->', className: 'code-arrow' });
      i += 3;
      continue;
    }

    if (ch === '-' && i + 1 < len && line[i + 1] === '>') {
      tokens.push({ text: '->', className: 'code-arrow' });
      i += 2;
      continue;
    }

    if (/[a-zA-Z_]/.test(ch)) {
      let j = i;
      while (j < len && /[a-zA-Z0-9_]/.test(line[j])) j++;
      const word = line.slice(i, j);
      let className: string | undefined;
      if (KEYWORDS.has(word)) {
        className = 'code-keyword';
      } else if (ENTITY_TYPES.has(word)) {
        className = 'code-entity';
      } else if (TYPES.has(word)) {
        className = 'code-type';
      }
      tokens.push({ text: word, className });
      i = j;
      continue;
    }

    if (/\d/.test(ch)) {
      let j = i;
      while (j < len && /[\d.]/.test(line[j])) j++;
      tokens.push({ text: line.slice(i, j), className: 'code-number' });
      i = j;
      continue;
    }

    if (/\s/.test(ch)) {
      let j = i;
      while (j < len && /\s/.test(line[j])) j++;
      tokens.push({ text: line.slice(i, j) });
      i = j;
      continue;
    }

    tokens.push({ text: ch });
    i++;
  }

  return tokens;
}

function highlight(code: string): React.ReactNode[] {
  const lines = code.split('\n');
  return lines.map((line, i) => {
    const tokens = tokenizeLine(line);
    return (
      <div key={i}>
        {tokens.map((t, j) =>
          t.className ? (
            <span key={j} className={t.className}>{t.text}</span>
          ) : (
            <span key={j}>{t.text}</span>
          )
        )}
      </div>
    );
  });
}

export default function CodeBlock({ code, language = 'tautcore', title }: CodeBlockProps) {
  return (
    <div className="code-block">
      {title && (
        <div className="code-block-header">
          <span className="code-block-dots">
            <span className="dot red" />
            <span className="dot yellow" />
            <span className="dot green" />
          </span>
          <span className="code-block-title">{title}</span>
          <span className="code-block-lang">{language}</span>
        </div>
      )}
      <pre className="code-block-body">
        <code>{highlight(code.trim())}</code>
      </pre>
    </div>
  );
}
