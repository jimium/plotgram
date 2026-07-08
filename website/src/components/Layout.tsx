import { Link, NavLink, Outlet } from 'react-router-dom';

function BrandIcon({ size = 28 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 50 50" fill="none">
      <defs>
        <linearGradient id="nav-bi" x1="0" y1="1" x2="1" y2="0">
          <stop offset="0%" stopColor="#7C3AED" />
          <stop offset="100%" stopColor="#06B6D4" />
        </linearGradient>
      </defs>
      <rect x="7" y="9" width="36" height="36" rx="8" fill="none" stroke="url(#nav-bi)" strokeWidth="2.5" />
      <path fill="url(#nav-bi)" d="M17.125 18H25A9 9 0 0 1 34 27v0a9 9 0 0 1-9 9h-7.875A1.125 1.125 0 0 1 16 34.875V19.125A1.125 1.125 0 0 1 17.125 18Z" />
    </svg>
  );
}

const NAV_ITEMS = [
  { to: '/', label: '首页', end: true },
  { to: '/docs/getting-started', label: '快速上手' },
  { to: '/docs/agent-guide', label: 'Agent 集成' },
  { to: '/showcase/', label: '示例画廊', external: true },
  { to: '/playground/', label: 'Playground', external: true },
  { to: '/docs/how-it-works', label: '技术揭秘' },
  { to: '/docs/trae-story', label: 'TRAE 实践' },
  { to: '/docs/faq', label: 'FAQ' },
];

export default function Layout() {
  return (
    <div className="site">
      <nav className="nav">
        <div className="container nav-inner">
          <Link to="/" className="nav-brand">
            <BrandIcon />
            <span className="nav-brand-name">Plotgram</span>
          </Link>
          <div className="nav-links">
            {NAV_ITEMS.map((item) =>
              item.external ? (
                <a key={item.to} href={item.to}>{item.label}</a>
              ) : (
                <NavLink
                  key={item.to}
                  to={item.to}
                  end={item.end}
                  className={({ isActive }) => (isActive ? 'nav-link-active' : '')}
                >
                  {item.label}
                </NavLink>
              ),
            )}
          </div>
          <a href="/playground/" className="btn btn-primary" style={{ padding: '8px 20px', fontSize: 14 }}>
            立即试用 →
          </a>
        </div>
      </nav>

      <main>
        <Outlet />
      </main>

      <footer className="footer">
        <div className="container footer-inner">
          <div className="footer-brand">
            <BrandIcon size={22} />
            <span>Plotgram</span>
          </div>
          <div className="footer-links">
            <a href="/playground/">Playground</a>
            <a href="/showcase/">Showcase</a>
            <a href="/docs/getting-started/">快速上手</a>
            <a href="/docs/agent-guide/">Agent 集成</a>
            <a href="/docs/how-it-works/">技术揭秘</a>
          </div>
        </div>
      </footer>
    </div>
  );
}
