/**
 * ToolCallTrace 工具调用轨迹（中间栏）
 *
 * 设计：左侧时间线竖线 + 序号圆形节点 + 工具名彩色标签 + 代码块展示入参/结果
 */

import { Tag, Tooltip, Spin, Empty } from 'antd';
import {
  CheckCircleFilled,
  CloseCircleFilled,
  LoadingOutlined,
  ToolOutlined,
  ApiOutlined,
  ThunderboltFilled,
  RocketOutlined,
} from '@ant-design/icons';
import type { ToolCallTraceItem } from '@hooks/useAgent';
import type { DiffResult } from '@agent/types';
import { DiffSummary } from './DiffSummary';

interface ToolCallTraceProps {
  items: ToolCallTraceItem[];
  running: boolean;
  lastDiff: DiffResult | null;
}

const TOOL_LABELS: Record<string, string> = {
  render: '渲染图表',
  validate: '校验语法',
  lint: '布局体检',
  parse: '解析 DSL',
  diff: '对比差异',
  apply_patch: '增量修改',
  layout_catalog: '查询布局目录',
};

const TOOL_THEMES: Record<string, { color: string; bg: string; icon: React.ReactNode }> = {
  render: {
    color: '#7c3aed',
    bg: 'linear-gradient(135deg, #ede9fe 0%, #ddd6fe 100%)',
    icon: <RocketOutlined />,
  },
  validate: {
    color: '#10b981',
    bg: 'linear-gradient(135deg, #d1fae5 0%, #a7f3d0 100%)',
    icon: <CheckCircleFilled />,
  },
  lint: {
    color: '#2563eb',
    bg: 'linear-gradient(135deg, #dbeafe 0%, #bfdbfe 100%)',
    icon: <ToolOutlined />,
  },
  apply_patch: {
    color: '#f59e0b',
    bg: 'linear-gradient(135deg, #fef3c7 0%, #fde68a 100%)',
    icon: <ThunderboltFilled />,
  },
  diff: {
    color: '#ec4899',
    bg: 'linear-gradient(135deg, #fce7f3 0%, #fbcfe8 100%)',
    icon: <ApiOutlined />,
  },
  parse: {
    color: '#06b6d4',
    bg: 'linear-gradient(135deg, #cffafe 0%, #a5f3fc 100%)',
    icon: <ApiOutlined />,
  },
  layout_catalog: {
    color: '#6b7280',
    bg: 'linear-gradient(135deg, #f3f4f6 0%, #e5e7eb 100%)',
    icon: <ApiOutlined />,
  },
};

export function ToolCallTrace({ items, running, lastDiff }: ToolCallTraceProps) {
  const successCount = items.filter((i) => i.status === 'success').length;
  const errorCount = items.filter((i) => i.status === 'error').length;

  return (
    <div className="trace-column">
      <div className="trace-column-header">
        <div className="trace-column-title">
          <span className="trace-column-icon">
            <ToolOutlined />
          </span>
          <span className="trace-column-title-text">执行轨迹</span>
          {running && (
            <span className="trace-running-badge">
              <Spin size="small" />
              <span>执行中</span>
            </span>
          )}
        </div>
        <div className="trace-column-stats">
          <span className="trace-stat trace-stat-total">
            <span className="trace-stat-num">{items.length}</span>
            <span className="trace-stat-label">调用</span>
          </span>
          {successCount > 0 && (
            <span className="trace-stat trace-stat-success">
              <CheckCircleFilled />
              <span className="trace-stat-num">{successCount}</span>
            </span>
          )}
          {errorCount > 0 && (
            <span className="trace-stat trace-stat-error">
              <CloseCircleFilled />
              <span className="trace-stat-num">{errorCount}</span>
            </span>
          )}
        </div>
      </div>

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
                <span className="trace-empty-text">
                  Agent 执行后这里会展示<br />工具调用轨迹
                </span>
              }
            />
          </div>
        ) : (
          <div className="trace-timeline">
            {items.map((item, idx) => (
              <ToolTraceRow key={item.id} item={item} index={idx + 1} />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

function ToolTraceRow({ item, index }: { item: ToolCallTraceItem; index: number }) {
  const label = TOOL_LABELS[item.toolName] ?? item.toolName;
  const theme = TOOL_THEMES[item.toolName] ?? {
    color: '#6b7280',
    bg: 'linear-gradient(135deg, #f3f4f6 0%, #e5e7eb 100%)',
    icon: <ApiOutlined />,
  };

  const statusConfig = {
    running: { icon: <LoadingOutlined spin />, color: '#1677ff', text: '执行中' },
    success: { icon: <CheckCircleFilled />, color: '#10b981', text: '成功' },
    error: { icon: <CloseCircleFilled />, color: '#ef4444', text: '失败' },
  }[item.status];

  return (
    <div className="trace-row">
      {/* 时间线节点 */}
      <div className="trace-row-marker" style={{ background: theme.bg, color: theme.color }}>
        {theme.icon}
        <span className="trace-row-marker-num">{index}</span>
      </div>

      <div className="trace-row-card">
        <div className="trace-row-header">
          <div className="trace-row-tool">
            <Tag color={theme.color} bordered={false} className="trace-row-tag">
              {item.toolName}
            </Tag>
            <span className="trace-row-label">{label}</span>
          </div>
          <span className="trace-row-status" style={{ color: statusConfig.color }}>
            {statusConfig.icon}
            <span className="trace-row-status-text">{statusConfig.text}</span>
          </span>
        </div>

        {item.argsPreview && (
          <div className="trace-row-args">
            <span className="trace-row-section-label">入参</span>
            <Tooltip title={item.argsPreview} placement="topLeft">
              <code className="trace-row-code">{item.argsPreview}</code>
            </Tooltip>
          </div>
        )}

        {item.resultPreview && (
          <div className="trace-row-result">
            <span className="trace-row-section-label">结果</span>
            <Tooltip title={item.resultPreview} placement="topLeft">
              <code
                className="trace-row-code"
                style={{ color: item.status === 'error' ? '#ef4444' : '#10b981' }}
              >
                {item.resultPreview}
              </code>
            </Tooltip>
          </div>
        )}
      </div>
    </div>
  );
}
