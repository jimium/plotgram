/**
 * ChatMessage 单条对话消息
 */

import { Typography, Tag, Space, Spin } from 'antd';
import { RobotOutlined, UserOutlined } from '@ant-design/icons';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import type { ChatMessage as ChatMessageType } from '@agent/types';
import { DiffSummary } from './DiffSummary';

const { Text } = Typography;

interface ChatMessageProps {
  message: ChatMessageType;
}

export function ChatMessage({ message }: ChatMessageProps) {
  if (message.role === 'system') {
    return (
      <div className="chat-msg chat-msg-system">
        <Text type="secondary" italic style={{ fontSize: 12 }}>
          {message.content}
        </Text>
      </div>
    );
  }

  const isUser = message.role === 'user';

  return (
    <div className={`chat-msg chat-msg-${message.role}`}>
      <div className="chat-msg-header">
        <Space size={6}>
          {isUser ? <UserOutlined /> : <RobotOutlined style={{ color: '#7c3aed' }} />}
          <Text strong style={{ fontSize: 12 }}>
            {isUser ? '我' : 'Agent'}
          </Text>
        </Space>
      </div>

      <div className="chat-msg-body">
        {isUser ? (
          <div className="chat-msg-text">{message.content}</div>
        ) : message.content ? (
          <div className="chat-msg-markdown">
            <ReactMarkdown remarkPlugins={[remarkGfm]}>{message.content}</ReactMarkdown>
          </div>
        ) : (
          <div className="chat-msg-thinking">
            <Spin size="small" />
            <span style={{ marginLeft: 8, color: '#999', fontSize: 12 }}>
              Agent 思考中...
            </span>
          </div>
        )}

        {message.toolCalls && message.toolCalls.length > 0 && (
          <Space size={[4, 4]} wrap style={{ marginBottom: 8 }}>
            {message.toolCalls.map((tc) => (
              <Tag key={tc.id} color="purple" style={{ fontSize: 11 }}>
                {tc.name}
              </Tag>
            ))}
          </Space>
        )}

        {message.diff && message.diff.changes.length > 0 && (
          <DiffSummary diff={message.diff} />
        )}

        {message.pendingChanges && (
          <Tag color="orange" style={{ marginTop: 8 }}>
            变更待确认
          </Tag>
        )}
      </div>
    </div>
  );
}
