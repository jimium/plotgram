/**
 * DiffSummary 变更摘要组件
 *
 * 将 Agent 的变更(DiffResult)渲染为可视化列表
 */

import { Tag, Typography, Space } from 'antd';
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

export function DiffSummary({ diff }: DiffSummaryProps) {
  const { stats, changes } = diff;

  return (
    <div className="diff-summary">
      <div className="diff-stats">
        <Space size={8}>
          <Tag color="success" icon={<PlusCircleOutlined />}>
            新增 {stats.added}
          </Tag>
          <Tag color="error" icon={<MinusCircleOutlined />}>
            删除 {stats.removed}
          </Tag>
          <Tag color="warning" icon={<EditOutlined />}>
            修改 {stats.modified}
          </Tag>
        </Space>
      </div>
      <div className="diff-changes">
        {changes.slice(0, 20).map((change, i) => (
          <DiffChangeRow key={i} change={change} />
        ))}
        {changes.length > 20 && (
          <Text type="secondary" style={{ fontSize: 11, marginTop: 4 }}>
            ...还有 {changes.length - 20} 条变更
          </Text>
        )}
      </div>
    </div>
  );
}

function DiffChangeRow({ change }: { change: Change }) {
  const config = {
    add: { color: '#52c41a', icon: <PlusCircleOutlined />, label: '新增' },
    remove: { color: '#ff4d4f', icon: <MinusCircleOutlined />, label: '删除' },
    modify: { color: '#faad14', icon: <EditOutlined />, label: '修改' },
  }[change.op];

  const pathStr = formatPath(change);

  return (
    <div className="diff-change-row" style={{ color: config.color }}>
      <span style={{ marginRight: 4 }}>{config.icon}</span>
      <Text code style={{ fontSize: 11, color: config.color }}>
        {pathStr}
      </Text>
      {change.description && (
        <Text type="secondary" style={{ fontSize: 11, marginLeft: 8 }}>
          {change.description}
        </Text>
      )}
    </div>
  );
}

function formatPath(change: Change): string {
  const { target, id, attr_key } = change.path;
  if (attr_key) {
    return `/${target}/${id}/${attr_key}`;
  }
  return `/${target}/${id}`;
}
