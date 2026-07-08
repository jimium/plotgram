interface CodeBlockProps {
  code: string;
  language?: string;
  title?: string;
}

function highlight(code: string): React.ReactNode[] {
  const lines = code.split('\n');
  return lines.map((line, i) => {
    const parts: React.ReactNode[] = [];
    let remaining = line;
    let key = 0;

    const commentMatch = remaining.match(/^(\s*)(\/\/.*)$/);
    if (commentMatch) {
      return (
        <div key={i}>
          <span className="code-indent">{commentMatch[1]}</span>
          <span className="code-comment">{commentMatch[2]}</span>
        </div>
      );
    }

    const tokens: { pattern: RegExp; className: string }[] = [
      { pattern: /\b(diagram|entity|layout|title|config|direction|type|semantic|status|owner)\b/g, className: 'code-keyword' },
      { pattern: /\b(flowchart|sequence|architecture|state|er|mindmap|left-to-right|top-to-bottom|right-to-left|bottom-to-top)\b/g, className: 'code-type' },
      { pattern: /"[^"]*"/g, className: 'code-string' },
      { pattern: /(-?>|-->|<->)/g, className: 'code-arrow' },
      { pattern: /\b(start|end|process|decision|database|service|gateway|browser|cache|user|server|client|api|auth)\b/g, className: 'code-entity' },
    ];

    let result = remaining;
    for (const { pattern, className } of tokens) {
      result = result.replace(pattern, (m) => `<span class="${className}">${m}</span>`);
    }

    parts.push(<span key={key++} dangerouslySetInnerHTML={{ __html: result }} />);

    return <div key={i}>{parts}</div>;
  });
}

export default function CodeBlock({ code, language = 'plotgram', title }: CodeBlockProps) {
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
