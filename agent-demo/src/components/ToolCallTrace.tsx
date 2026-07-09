/**
 * ToolCallTrace 工具调用轨迹组件
 *
 * 演示场景的"杀手锏"：让用户直观看到 Agent 的思考-执行循环。
 * 每一项展示：工具名 → 参数摘要 → 结果摘要 + 状态徽标。
 * 运行中显示 Spinner，完成后按 success/error 着色。
 */

import { Tag, Tooltip, Typography, Spin } from 'antd';
import {
  CheckCircleFilled,
  CloseCircleFilled,
  LoadingOutlined,
  ToolOutlined,
} from '@ant-design/icons';
import type { ToolCallTraceItem } from '@hooks/useAgent';

const { Text } = Typography;

interface ToolCallTraceProps {
  items: ToolCallTraceItem[];
  running: boolean;
}

/** 工具名 → 中文标签映射 */
const TOOL_LABELS: Record<string, string> = {
  render: '渲染',
  validate: '校验',
  parse: '解析',
  diff: '对比',
  apply_patch: '打补丁',
  layout_catalog: '查询布局',
};

export function ToolCallTrace({ items, running }: ToolCallTraceProps) {
  return (
    <div className="tool-trace">
      <div className="tool-trace-header">
        <ToolOutlined style={{ marginRight: 6, color: '#7c3aed' }} />
        <Text strong style={{ fontSize: 12 }}>
          工具调用轨迹
        </Text>
        <Text type="secondary" style={{ fontSize: 11, marginLeft: 8 }}>
          {items.length} 次{running ? ' · 执行中' : ' · 已完成'}
        </Text>
      </div>

      <div className="tool-trace-list">
        {items.map((item, idx) => (
          <ToolTraceRow key={item.id} item={item} index={idx + 1} />
        ))}
      </div>
    </div>
  );
}

function ToolTraceRow({ item, index }: { item: ToolCallTraceItem; index: number }) {
  const label = TOOL_LABELS[item.toolName] ?? item.toolName;

  const statusConfig = {
    running: {
      icon: <LoadingOutlined />,
      color: '#1677ff' as const,
      text: '执行中',
    },
    success: {
      icon: <CheckCircleFilled />,
      color: '#52c41a' as const,
      text: '成功',
    },
    error: {
      icon: <CloseCircleFilled />,
      color: '#ff4d4f' as const,
      text: '失败',
    },
  }[item.status];

  return (
    <div className="tool-trace-row">
      <div className="tool-trace-row-header">
        <span className="tool-trace-index">#{index}</span>
        <Tag color="purple" style={{ fontSize: 11, margin: 0 }}>
          {item.toolName}
        </Tag>
        <Text style={{ fontSize: 11, color: '#666' }}>{label}</Text>
        <span style={{ marginLeft: 'auto', display: 'flex', alignItems: 'center', gap: 4 }}>
          {item.status === 'running' && <Spin size="small" />}
          <Text style={{ fontSize: 11, color: statusConfig.color }}>
            {statusConfig.icon}
            <span style={{ marginLeft: 4 }}>{statusConfig.text}</span>
          </Text>
        </span>
      </div>

      {item.argsPreview && (
        <div className="tool-trace-args">
          <Text type="secondary" style={{ fontSize: 10 }}>
            入参:
          </Text>
          <code className="tool-trace-code">{item.argsPreview}</code>
        </div>
      )}

      {item.resultPreview && (
        <div className="tool-trace-result">
          <Text type="secondary" style={{ fontSize: 10 }}>
            结果:
          </Text>
          <Tooltip title={item.resultPreview} placement="topLeft">
            <code
              className="tool-trace-code"
              style={{
                color: item.status === 'error' ? '#ff4d4f' : '#52c41a',
              }}
            >
              {item.resultPreview}
            </code>
          </Tooltip>
        </div>
      )}
    </div>
  );
}
