/**
 * ChatPanel 对话面板
 *
 * Studio 的核心交互入口,用户在此与 Agent 对话
 */

import { useCallback, useEffect, useRef, useState } from 'react';
import { Input, Button, Alert, Empty, Typography } from 'antd';
import { SendOutlined, StopOutlined } from '@ant-design/icons';
import type { ChatMessage as ChatMessageType } from '@agent/types';
import { ChatMessage } from './ChatMessage';

const { TextArea } = Input;
const { Text } = Typography;

interface ChatPanelProps {
  messages: ChatMessageType[];
  isRunning: boolean;
  error: string | null;
  onSend: (text: string) => void;
  onAbort: () => void;
  onClearError: () => void;
}

export function ChatPanel({
  messages,
  isRunning,
  error,
  onSend,
  onAbort,
  onClearError,
}: ChatPanelProps) {
  const [input, setInput] = useState('');
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const scrollContainerRef = useRef<HTMLDivElement>(null);
  // 标记用户是否手动滚动离开底部
  const userScrolledUpRef = useRef(false);

  // 智能滚动:只在用户处于底部附近时自动滚动
  useEffect(() => {
    const container = scrollContainerRef.current;
    if (!container) return;
    const handleScroll = () => {
      const { scrollTop, scrollHeight, clientHeight } = container;
      // 距离底部小于 80px 视为"在底部"
      userScrolledUpRef.current = scrollHeight - scrollTop - clientHeight > 80;
    };
    container.addEventListener('scroll', handleScroll);
    return () => container.removeEventListener('scroll', handleScroll);
  }, []);

  useEffect(() => {
    // 只在用户未手动上滚时自动滚动到底部
    if (!userScrolledUpRef.current) {
      messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
    }
  }, [messages, isRunning]);

  const handleSend = useCallback(() => {
    const text = input.trim();
    if (!text || isRunning) return;
    onSend(text);
    setInput('');
    // 发送后重置滚动标记,确保自动滚动到底部
    userScrolledUpRef.current = false;
  }, [input, isRunning, onSend]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
      if (e.key === 'Enter' && !e.shiftKey) {
        e.preventDefault();
        handleSend();
      }
    },
    [handleSend],
  );

  return (
    <div className="chat-panel">
      <div ref={scrollContainerRef} className="chat-messages">
        {messages.length === 0 && !isRunning && (
          <Empty
            image={Empty.PRESENTED_IMAGE_SIMPLE}
            description={
              <span style={{ fontSize: 13 }}>
                用自然语言描述你想要的图表
                <br />
                例如"画一个微服务架构图"
              </span>
            }
            style={{ marginTop: 80 }}
          />
        )}

        {messages.map((msg) => (
          <ChatMessage key={msg.id} message={msg} />
        ))}

        {error && (
          <Alert
            type="error"
            message="执行出错"
            description={error}
            showIcon
            closable
            onClose={onClearError}
            style={{ margin: '8px 0' }}
          />
        )}

        <div ref={messagesEndRef} />
      </div>

      <div className="chat-input-area">
        <div className="chat-input-wrapper">
          <TextArea
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder={
              isRunning
                ? 'Agent 执行中,请等待或点击中止...'
                : '描述你想要的图表,如:画一个用户认证流程图'
            }
            disabled={isRunning}
            autoSize={{ minRows: 1, maxRows: 4 }}
            style={{ resize: 'none' }}
          />
          {isRunning ? (
            <Button
              danger
              icon={<StopOutlined />}
              onClick={onAbort}
              className="chat-send-btn"
            >
              中止
            </Button>
          ) : (
            <Button
              type="primary"
              icon={<SendOutlined />}
              onClick={handleSend}
              disabled={!input.trim()}
              className="chat-send-btn"
            >
              发送
            </Button>
          )}
        </div>
        <div className="chat-input-hint">
          <Text type="secondary" style={{ fontSize: 11 }}>
            Enter 发送 · Shift+Enter 换行
          </Text>
        </div>
      </div>
    </div>
  );
}
