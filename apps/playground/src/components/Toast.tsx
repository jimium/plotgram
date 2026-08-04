import { useEffect } from 'react';

export interface ToastMessage {
  id: number;
  text: string;
  kind: 'info' | 'success' | 'error';
}

interface ToastProps {
  toast: ToastMessage | null;
  onDismiss: () => void;
}

export function Toast({ toast, onDismiss }: ToastProps) {
  useEffect(() => {
    if (!toast) return;
    const timer = window.setTimeout(onDismiss, 2600);
    return () => window.clearTimeout(timer);
  }, [toast, onDismiss]);

  if (!toast) return null;

  return (
    <div className={`toast toast-${toast.kind}`} role="status">
      {toast.text}
    </div>
  );
}
