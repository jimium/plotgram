/**
 * ChatMessage 单条对话消息
 *
 * 设计：
 *   - 渐变圆形头像（User 紫→粉 / Agent 蓝→青）
 *   - Agent 消息左对齐白色卡片 + 细边阴影
 *   - User 消息右对齐 + 渐变气泡
 *   - Markdown 标题/列表/代码/引用精细排版
 */

import { Spin, Space } from 'antd';
import {
  UserOutlined,
  RobotOutlined,
  CodeOutlined,
} from '@ant-design/icons';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import type { ChatMessage as ChatMessageType } from '@agent/types';
import { DiffSummary } from './DiffSummary';

interface ChatMessageProps {
  message: ChatMessageType;
}

export function ChatMessage({ message }: ChatMessageProps) {
  if (message.role === 'system') {
    return (
      <div className="chat-msg chat-msg-system">
        <div className="chat-msg-system-divider">{message.content}</div>
      </div>
    );
  }

  const isUser = message.role === 'user';

  return (
    <div className={`chat-msg chat-msg-${message.role}`}>
      <div className={`chat-msg-avatar chat-msg-avatar-${message.role}`}>
        {isUser ? <UserOutlined /> : <RobotOutlined />}
      </div>

      <div className="chat-msg-column">
        <div className="chat-msg-header">
          <span className="chat-msg-name">
            {isUser ? '我' : 'Tautcore Agent'}
          </span>
          <span className="chat-msg-role">
            {isUser ? '提问者' : 'AI 助手'}
          </span>
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
              <span className="chat-msg-thinking-text">Agent 思考中…</span>
            </div>
          )}

          {message.toolCalls && message.toolCalls.length > 0 && (
            <div className="chat-msg-tools">
              <Space size={[6, 6]} wrap>
                {message.toolCalls.map((tc) => (
                  <span key={tc.id} className="chat-msg-tool">
                    <CodeOutlined />
                    {tc.name}
                  </span>
                ))}
              </Space>
            </div>
          )}

          {message.diff && message.diff.changes.length > 0 && (
            <div className="chat-msg-diff">
              <DiffSummary diff={message.diff} />
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
