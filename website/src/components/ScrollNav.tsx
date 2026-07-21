import { useEffect, useState, useCallback } from 'react';

const NAV_ITEMS = [
  { id: 'hero', label: '首页' },
  { id: 'overview', label: '项目概览' },
  { id: 'features', label: '核心特性' },
  { id: 'algorithms', label: '核心引擎' },
  { id: 'diagram-types', label: '图表类型' },
  { id: 'comparison', label: '方案对比' },
];

export default function ScrollNav() {
  const [activeId, setActiveId] = useState('hero');

  useEffect(() => {
    const sections = NAV_ITEMS
      .map((item) => document.getElementById(item.id))
      .filter((el): el is HTMLElement => el !== null);

    if (sections.length === 0) return;

    const observer = new IntersectionObserver(
      (entries) => {
        const visible = entries
          .filter((e) => e.isIntersecting)
          .sort((a, b) => b.intersectionRatio - a.intersectionRatio);
        if (visible.length > 0) {
          setActiveId(visible[0].target.id);
        }
      },
      {
        rootMargin: '-20% 0px -60% 0px',
        threshold: [0, 0.1, 0.25, 0.5, 0.75, 1],
      },
    );

    sections.forEach((s) => observer.observe(s));
    return () => observer.disconnect();
  }, []);

  const handleClick = useCallback((id: string) => {
    const el = document.getElementById(id);
    if (!el) return;
    el.scrollIntoView({ behavior: 'smooth', block: 'start' });
  }, []);

  return (
    <nav className="scroll-nav" aria-label="页面导航">
      {NAV_ITEMS.map((item) => (
        <button
          key={item.id}
          className={`scroll-nav-dot ${activeId === item.id ? 'active' : ''}`}
          onClick={() => handleClick(item.id)}
          aria-label={item.label}
          title={item.label}
        >
          <span className="scroll-nav-dot-inner" />
          <span className="scroll-nav-label">{item.label}</span>
        </button>
      ))}
    </nav>
  );
}
