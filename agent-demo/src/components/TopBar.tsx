/**
 * TopBar 顶栏组件（演示版）
 *
 * 设计语言：
 *   - 使用 brand 标准 logo（assets/brand/logo-icon-32.svg）
 *   - 玻璃态分割线 + 状态指示灯
 *   - 状态徽标彩色 chip 化
 */

import { Button, Tooltip, Space, Typography } from 'antd';
import {
  CheckCircleFilled,
  ExclamationCircleFilled,
  LoadingOutlined,
  ThunderboltFilled,
  BookOutlined,
  GithubOutlined,
  HomeOutlined,
} from '@ant-design/icons';

const { Text } = Typography;

interface TopBarProps {
  version: string;
  wasmReady: boolean;
  wasmError: string | null;
  isAgentRunning: boolean;
  onOpenDocs?: () => void;
}

/**
 * 标准品牌 logo 路径：开发走 vite dev server，生产走 CDN。
 * 与 wasm.ts 中的 wasmAssetBase 同样的拼接规则。
 */
function brandLogoUrl(): string {
  const cdn = import.meta.env.VITE_CDN_BASE || '';
  const base = cdn || import.meta.env.BASE_URL || '/';
  return `${base.replace(/\/?$/, '/')}assets/brand/logo-icon-32.svg`;
}

export function TopBar({
  version,
  wasmReady,
  wasmError,
  isAgentRunning,
  onOpenDocs,
}: TopBarProps) {
  return (
    <div className="studio-topbar">
      <div className="topbar-brand">
        <img
          src={brandLogoUrl()}
          width="30"
          height="30"
          alt="Plotgram"
          className="topbar-logo"
        />
        <div className="topbar-titles">
          <Text strong className="topbar-title">
            Plotgram Agent
          </Text>
          <Text className="topbar-subtitle">对话即画图 · AI 原生图表引擎</Text>
        </div>
      </div>

      <div className="topbar-status">
        {wasmError ? (
          <StatusChip
            tone="error"
            icon={<ExclamationCircleFilled />}
            text="WASM 加载失败"
          />
        ) : wasmReady ? (
          <StatusChip
            tone="success"
            icon={<CheckCircleFilled />}
            text={`WASM ${version} 就绪`}
            pulse
          />
        ) : (
          <StatusChip
            tone="processing"
            icon={<LoadingOutlined spin />}
            text="WASM 加载中"
          />
        )}
        {isAgentRunning && (
          <StatusChip
            tone="processing"
            icon={<ThunderboltFilled />}
            text="Agent 执行中"
            pulse
          />
        )}
      </div>

      <Space className="topbar-actions">
        <Tooltip title="返回 Plotgram 主站">
          <Button
            type="text"
            icon={<HomeOutlined />}
            href="/"
          >
            主站
          </Button>
        </Tooltip>
        <Tooltip title="查看使用文档">
          <Button
            type="text"
            icon={<BookOutlined />}
            onClick={onOpenDocs}
          >
            文档
          </Button>
        </Tooltip>
        <Tooltip title="Plotgram 仓库">
          <Button
            type="text"
            icon={<GithubOutlined />}
            onClick={() => window.open('https://github.com/plotgram/plotgram', '_blank', 'noopener')}
          />
        </Tooltip>
      </Space>
    </div>
  );
}

function StatusChip({
  tone,
  icon,
  text,
  pulse,
}: {
  tone: 'success' | 'error' | 'processing';
  icon: React.ReactNode;
  text: string;
  pulse?: boolean;
}) {
  return (
    <div className={`status-chip status-chip-${tone}${pulse ? ' status-chip-pulse' : ''}`}>
      <span className="status-chip-icon">{icon}</span>
      <span className="status-chip-text">{text}</span>
    </div>
  );
}
