/**
 * ToolCallTrace 工具调用轨迹（中间栏）
 *
 * 独立列展示 Agent 的思考-执行循环：
 * - 顶部统计：总调用次数 / 成功 / 失败
 * - 可滚动列表：每项展示工具名、入参、结果、状态
 * - 运行中实时刷新
 */

import { Tag, Tooltip, Typography, Spin, Empty, Badge } from 'antd';
import {
  CheckCircleFilled,
  CloseCircleFilled,
  LoadingOutlined,
  ToolOutlined,
} from '@ant-design/icons';
import type { ToolCallTraceItem } from '@hooks/useAgent';
import type { DiffResult } from '@agent/types';
import { DiffSummary } from './DiffSummary';

const { Text } = Typography;

interface ToolCallTraceProps {
  items: ToolCallTraceItem[];
  running: boolean;
  lastDiff: DiffResult | null;
}

/** 工具名 → 中文标签映射 */
const TOOL_LABELS: Record<string, string> = {
  render: '渲染图表',
  validate: '校验语法',
  parse: '解析 DSL',
  diff: '对比差异',
  apply_patch: '增量修改',
  layout_catalog: '查询布局目录',
};

/** 工具名 → 颜色 */
const TOOL_COLORS: Record<string, string> = {
  render: 'geekblue',
  validate: 'green',
  apply_patch: 'orange',
  diff: 'purple',
  parse: 'cyan',
  layout_catalog: 'default',
};

export function ToolCallTrace({ items, running, lastDiff }: ToolCallTraceProps) {
  const successCount = items.filter((i) => i.status === 'success').length;
  const errorCount = items.filter((i) => i.status === 'error').length;

  return (
    <div className="trace-column">
      <div className="trace-column-header">
        <div className="trace-column-title">
          <ToolOutlined style={{ marginRight: 6, color: '#7c3aed' }} />
          <Text strong style={{ fontSize: 13 }}>
            执行轨迹
          </Text>
          {running && (
            <Badge status="processing" text={<Text type="secondary" style={{ fontSize: 11 }}>执行中</Text>} />
          )}
        </div>
        <div className="trace-column-stats">
          <span className="trace-stat">{items.length} 次</span>
          {successCount > 0 && <span className="trace-stat trace-stat-success">✓{successCount}</span>}
          {errorCount > 0 && <span className="trace-stat trace-stat-error">✗{errorCount}</span>}
        </div>
      </div>

      {/* 变更摘要 */}
      {lastDiff && lastDiff.changes.length > 0 && (
        <div className="trace-diff-section">
          <DiffSummary diff={lastDiff} />
        </div>
      )}

      <div className="trace-column-list">
        {items.length === 0 && !running ? (
          <div className="trace-empty">
            <Empty
              image={Empty.PRESENTED_IMAGE_SIMPLE}
              description={
                <Text type="secondary" style={{ fontSize: 12 }}>
                  Agent 执行后这里会展示工具调用轨迹
                </Text>
              }
            />
          </div>
        ) : (
          items.map((item, idx) => (
            <ToolTraceRow key={item.id} item={item} index={idx + 1} />
          ))
        )}
      </div>
    </div>
  );
}

function ToolTraceRow({ item, index }: { item: ToolCallTraceItem; index: number }) {
  const label = TOOL_LABELS[item.toolName] ?? item.toolName;
  const color = TOOL_COLORS[item.toolName] ?? 'default';

  const statusConfig = {
    running: { icon: <LoadingOutlined />, color: '#1677ff', text: '执行中' },
    success: { icon: <CheckCircleFilled />, color: '#52c41a', text: '成功' },
    error: { icon: <CloseCircleFilled />, color: '#ff4d4f', text: '失败' },
  }[item.status];

  return (
    <div className={`trace-row trace-row-${item.status}`}>
      <div className="trace-row-header">
        <span className="trace-row-index">#{index}</span>
        <Tag color={color} style={{ fontSize: 11, margin: 0 }}>
          {item.toolName}
        </Tag>
        <Text style={{ fontSize: 11, color: '#666' }}>{label}</Text>
        <span className="trace-row-status" style={{ color: statusConfig.color }}>
          {item.status === 'running' && <Spin size="small" style={{ marginRight: 4 }} />}
          {statusConfig.icon}
          <span style={{ marginLeft: 4, fontSize: 11 }}>{statusConfig.text}</span>
        </span>
      </div>

      {item.argsPreview && (
        <div className="trace-row-args">
          <Text type="secondary" style={{ fontSize: 10 }}>入参</Text>
          <Tooltip title={item.argsPreview} placement="topLeft">
            <code className="trace-row-code">{item.argsPreview}</code>
          </Tooltip>
        </div>
      )}

      {item.resultPreview && (
        <div className="trace-row-result">
          <Text type="secondary" style={{ fontSize: 10 }}>结果</Text>
          <Tooltip title={item.resultPreview} placement="topLeft">
            <code
              className="trace-row-code"
              style={{ color: item.status === 'error' ? '#ff4d4f' : '#52c41a' }}
            >
              {item.resultPreview}
            </code>
          </Tooltip>
        </div>
      )}
    </div>
  );
}
