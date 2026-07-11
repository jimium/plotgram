import { ReactNode } from 'react';

interface DocPageProps {
  title: string;
  description?: string;
  sidebar?: { label: string; items: { to: string; label: string; active?: boolean }[] }[];
  children: ReactNode;
}

export default function DocPage({ title, description, sidebar, children }: DocPageProps) {
  return (
    <div className="docs-page">
      <div className="container docs-container">
        {sidebar && (
          <aside className="docs-sidebar">
            {sidebar.map((section) => (
              <div key={section.label} className="docs-sidebar-section">
                <div className="docs-sidebar-label">{section.label}</div>
                {section.items.map((item) => (
                  <a
                    key={item.to}
                    href={item.to}
                    className={`docs-sidebar-link ${item.active ? 'active' : ''}`}
                  >
                    {item.label}
                  </a>
                ))}
              </div>
            ))}
          </aside>
        )}
        <article className="docs-content">
          <header className="docs-header">
            <h1>{title}</h1>
            {description && <p className="docs-description">{description}</p>}
          </header>
          {children}
        </article>
      </div>
    </div>
  );
}

export const DOCS_SIDEBAR = [
  {
    label: '开始使用',
    items: [
      { to: '/docs/getting-started/', label: '快速上手', active: false },
      { to: '/docs/faq/', label: '常见问题', active: false },
    ],
  },
  {
    label: 'AI 集成',
    items: [
      { to: '/docs/agent-guide/', label: 'Agent 集成指南', active: false },
    ],
  },
  {
    label: '深度阅读',
    items: [
      { to: '/docs/how-it-works/', label: '技术揭秘', active: false },
      { to: '/docs/trae-story/', label: 'TRAE 开发实践', active: false },
      { to: '/docs/roadmap/', label: '路线图', active: false },
    ],
  },
];
