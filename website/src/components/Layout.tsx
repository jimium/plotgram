import { Link, NavLink, Outlet } from 'react-router-dom';

function BrandIcon({ size = 28 }: { size?: number }) {
  return (
    <img
      className="nav-brand-icon"
      src="/assets/brand/logo-icon-32.svg"
      width={size}
      height={size}
      alt=""
      aria-hidden="true"
    />
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
