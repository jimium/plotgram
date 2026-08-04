import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { redirectLegacyPlaygroundPath } from './lib/legacyRedirect';
import './index.css';
import App from './App';

redirectLegacyPlaygroundPath();

const rootEl = document.getElementById('root');
if (!rootEl) throw new Error('root element not found');

createRoot(rootEl).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
