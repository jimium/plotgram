/**
 * TopBar 顶栏组件（演示版）
 *
 * 与 studio 差异：
 *   - 标题改为 "Plotgram Agent · 对话即画图"
 *   - 移除 LLM 配置按钮（演示无需用户配置 Key）
 *   - 移除 DSL Viewer 入口（演示不暴露源码）
 *   - 保留 WASM 状态徽标 + 导出 SVG
 */

import { Button, Tooltip, Tag, Space, Typography } from 'antd';
import {
  DownloadOutlined,
  CheckCircleFilled,
  ExclamationCircleFilled,
  LoadingOutlined,
  ThunderboltFilled,
  GithubOutlined,
} from '@ant-design/icons';

const { Text } = Typography;

interface TopBarProps {
  version: string;
  wasmReady: boolean;
  wasmError: string | null;
  isAgentRunning: boolean;
  canExport: boolean;
  onExport: () => void;
}

export function TopBar({
  version,
  wasmReady,
  wasmError,
  isAgentRunning,
  canExport,
  onExport,
}: TopBarProps) {
  return (
    <div className="studio-topbar">
      <div className="topbar-brand">
        <Text strong style={{ color: '#7c3aed', fontSize: 15 }}>
          Plotgram Agent
        </Text>
        <Text type="secondary" style={{ fontSize: 12, marginLeft: 8 }}>
          对话即画图
        </Text>
      </div>

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
        <Tooltip title="Plotgram 是开源图表 DSL，对话即可生成图表">
          <Button type="text" icon={<GithubOutlined />} disabled>
            开源
          </Button>
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
