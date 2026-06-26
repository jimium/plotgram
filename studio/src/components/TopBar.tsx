/**
 * TopBar 顶栏组件
 */

import { Button, Tooltip, Tag, Space, Typography } from 'antd';
import {
  DownloadOutlined,
  CodeOutlined,
  SettingOutlined,
  CheckCircleFilled,
  ExclamationCircleFilled,
  LoadingOutlined,
  ThunderboltFilled,
} from '@ant-design/icons';

const { Text } = Typography;

interface TopBarProps {
  version: string;
  wasmReady: boolean;
  wasmError: string | null;
  isAgentRunning: boolean;
  canExport: boolean;
  onToggleDslViewer: () => void;
  onExport: () => void;
  onOpenConfig: () => void;
}

export function TopBar({
  version,
  wasmReady,
  wasmError,
  isAgentRunning,
  canExport,
  onToggleDslViewer,
  onExport,
  onOpenConfig,
}: TopBarProps) {
  return (
    <div className="studio-topbar">
      <Text strong style={{ color: '#7c3aed', fontSize: 15 }}>
        Drawify Studio
      </Text>

      <Space size="small" style={{ marginLeft: 16 }}>
        {wasmError ? (
          <Tag icon={<ExclamationCircleFilled />} color="error">
            WASM 加载失败
          </Tag>
        ) : wasmReady ? (
          <Tag icon={<CheckCircleFilled />} color="success">
            WASM {version}
          </Tag>
        ) : (
          <Tag icon={<LoadingOutlined />} color="processing">
            WASM 加载中
          </Tag>
        )}
        {isAgentRunning && (
          <Tag icon={<ThunderboltFilled />} color="processing">
            Agent 执行中
          </Tag>
        )}
      </Space>

      <Space style={{ marginLeft: 'auto' }}>
        <Tooltip title="查看当前 DSL 源码">
          <Button
            type="text"
            icon={<CodeOutlined />}
            onClick={onToggleDslViewer}
          >
            DSL
          </Button>
        </Tooltip>
        <Tooltip title="LLM 配置">
          <Button
            type="text"
            icon={<SettingOutlined />}
            onClick={onOpenConfig}
          />
        </Tooltip>
        <Button
          type="primary"
          icon={<DownloadOutlined />}
          onClick={onExport}
          disabled={!canExport}
        >
          导出 SVG
        </Button>
      </Space>
    </div>
  );
}
