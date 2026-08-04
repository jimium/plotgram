import { useRef, type PointerEvent as ReactPointerEvent } from 'react';

interface ResizeHandleProps {
  onResize: (deltaX: number) => void;
}

export function ResizeHandle({ onResize }: ResizeHandleProps) {
  const lastX = useRef<number | null>(null);

  const onPointerDown = (e: ReactPointerEvent<HTMLDivElement>) => {
    e.preventDefault();
    lastX.current = e.clientX;
    (e.target as HTMLElement).setPointerCapture(e.pointerId);
    document.body.classList.add('resizing');
  };

  const onPointerMove = (e: ReactPointerEvent<HTMLDivElement>) => {
    if (lastX.current === null) return;
    const delta = e.clientX - lastX.current;
    lastX.current = e.clientX;
    onResize(delta);
  };

  const onPointerUp = (e: ReactPointerEvent<HTMLDivElement>) => {
    lastX.current = null;
    (e.target as HTMLElement).releasePointerCapture(e.pointerId);
    document.body.classList.remove('resizing');
  };

  return (
    <div
      className="resize-handle"
      role="separator"
      aria-orientation="vertical"
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
    />
  );
}
