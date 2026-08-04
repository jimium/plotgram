/**
 * DiffSummary 变更摘要组件
 *
 * 设计：顶部 pill 风格统计 + 列表展示各变更路径
 */

import { Typography } from 'antd';
import {
  PlusCircleOutlined,
  MinusCircleOutlined,
  EditOutlined,
} from '@ant-design/icons';
import type { DiffResult, Change } from '@agent/types';

const { Text } = Typography;

interface DiffSummaryProps {
  diff: DiffResult;
}

const OP_THEMES: Record<
  Change['op'],
  { color: string; bg: string; border: string; icon: React.ReactNode; label: string }
> = {
  add: {
    color: '#10b981',
    bg: 'linear-gradient(135deg, #ecfdf5 0%, #d1fae5 100%)',
    border: '#a7f3d0',
    icon: <PlusCircleOutlined />,
    label: '新增',
  },
  remove: {
    color: '#ef4444',
    bg: 'linear-gradient(135deg, #fef2f2 0%, #fee2e2 100%)',
    border: '#fecaca',
    icon: <MinusCircleOutlined />,
    label: '删除',
  },
  modify: {
    color: '#f59e0b',
    bg: 'linear-gradient(135deg, #fffbeb 0%, #fef3c7 100%)',
    border: '#fde68a',
    icon: <EditOutlined />,
    label: '修改',
  },
};

export function DiffSummary({ diff }: DiffSummaryProps) {
  const { stats, changes } = diff;

  return (
    <div className="diff-summary">
      <div className="diff-summary-header">本次变更</div>
      <div className="diff-stats">
        <div className="diff-stat diff-stat-add">
          <PlusCircleOutlined />
          <span className="diff-stat-num">{stats.added}</span>
          <span className="diff-stat-label">新增</span>
        </div>
        <div className="diff-stat diff-stat-remove">
          <MinusCircleOutlined />
          <span className="diff-stat-num">{stats.removed}</span>
          <span className="diff-stat-label">删除</span>
        </div>
        <div className="diff-stat diff-stat-modify">
          <EditOutlined />
          <span className="diff-stat-num">{stats.modified}</span>
          <span className="diff-stat-label">修改</span>
        </div>
      </div>
      <div className="diff-changes">
        {changes.slice(0, 8).map((change, i) => (
          <DiffChangeRow key={i} change={change} />
        ))}
        {changes.length > 8 && (
          <Text type="secondary" className="diff-changes-more">
            ... 还有 {changes.length - 8} 条变更
          </Text>
        )}
      </div>
    </div>
  );
}

function DiffChangeRow({ change }: { change: Change }) {
  const theme = OP_THEMES[change.op];
  const pathStr = formatPath(change);

  return (
    <div
      className="diff-change-row"
      style={{ background: theme.bg, borderColor: theme.border }}
    >
      <span className="diff-change-icon" style={{ color: theme.color }}>
        {theme.icon}
      </span>
      <span className="diff-change-path">{pathStr}</span>
      <span className="diff-change-label" style={{ color: theme.color }}>
        {theme.label}
      </span>
    </div>
  );
}

function formatPath(change: Change): string {
  const { target, id, attr_key } = change.path;
  if (attr_key) {
    return `${target}/${id}/${attr_key}`;
  }
  return `${target}/${id}`;
}
