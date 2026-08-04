/**
 * DslViewer DSL 只读查看器
 *
 * 展示当前生效的 DSL 源码,不提供编辑(与 Playground 划清边界)
 * 使用 antd Drawer 从底部滑出
 */

import { Drawer, Typography, Empty, Button, App } from 'antd';
import { CopyOutlined } from '@ant-design/icons';
import { copyText } from '@lib/exportImage';

const { Text } = Typography;

interface DslViewerProps {
  open: boolean;
  source: string;
  onClose: () => void;
}

export function DslViewer({ open, source, onClose }: DslViewerProps) {
  const { message } = App.useApp();

  const handleCopy = async () => {
    if (!source) return;
    try {
      await copyText(source);
      message.success('DSL 已复制到剪贴板');
    } catch {
      message.error('复制失败,请手动选择文本');
    }
  };

  return (
    <Drawer
      title="当前 DSL 源码(只读)"
      placement="bottom"
      open={open}
      onClose={onClose}
      height={320}
      styles={{ body: { padding: 16, overflow: 'auto' } }}
      extra={
        source ? (
          <Button
            size="small"
            type="text"
            icon={<CopyOutlined />}
            onClick={handleCopy}
          >
            复制
          </Button>
        ) : undefined
      }
    >
      {source ? (
        <pre
          style={{
            margin: 0,
            padding: 12,
            background: '#f5f5f5',
            borderRadius: 6,
            fontFamily: 'JetBrains Mono, SF Mono, Monaco, Menlo, monospace',
            fontSize: 12,
            lineHeight: 1.6,
            whiteSpace: 'pre',
            overflow: 'auto',
          }}
        >
          {source}
        </pre>
      ) : (
        <Empty
          image={Empty.PRESENTED_IMAGE_SIMPLE}
          description={<Text type="secondary">暂无图表,请在对话区输入需求生成</Text>}
        />
      )}
    </Drawer>
  );
}
