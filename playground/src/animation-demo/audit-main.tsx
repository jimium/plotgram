import React from 'react';
import ReactDOM from 'react-dom/client';
import '../index.css';
import './demo.css';
import AuditApp from './AuditApp';

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <AuditApp />
  </React.StrictMode>
);
