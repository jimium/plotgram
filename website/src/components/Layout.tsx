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
  { to: '/docs/trae-story', label: 'TRAE 开发实践' },
  { to: '/docs/roadmap', label: '路线图' },
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
            {NAV_ITEMS.map((item) => (
              <NavLink
                key={item.to}
                to={item.to}
                end={item.end}
                className={({ isActive }) => (isActive ? 'nav-link-active' : '')}
              >
                {item.label}
              </NavLink>
            ))}
          </div>
          <div className="nav-cta-group">
            <a href="/playground/" className="nav-cta nav-cta-primary" target="_blank" rel="noopener noreferrer">
              Playground
            </a>
            <a href="/showcase/" className="nav-cta" target="_blank" rel="noopener noreferrer">
              Showcase
            </a>
            <a href="/agent/" className="nav-cta nav-cta-accent" target="_blank" rel="noopener noreferrer">
              Agent
            </a>
          </div>
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
            <a href="/playground/" target="_blank" rel="noopener noreferrer">Playground</a>
            <a href="/showcase/" target="_blank" rel="noopener noreferrer">Showcase</a>
            <a href="/agent/" target="_blank" rel="noopener noreferrer">Agent</a>
            <a href="/docs/getting-started/">快速上手</a>
            <a href="/docs/agent-guide/">Agent 集成</a>
            <a href="/docs/how-it-works/">技术揭秘</a>
            <a href="/docs/trae-story/">TRAE 开发实践</a>
            <a href="/docs/roadmap/">路线图</a>
            <a href="/docs/faq/">FAQ</a>
          </div>
        </div>
      </footer>
    </div>
  );
}
