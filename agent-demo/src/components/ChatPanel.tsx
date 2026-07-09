/**
 * ChatPanel 对话面板
 *
 * 演示版在 studio 基础上：
 *   - 顶部嵌入 ExamplePicker 一键示例（无消息时显示）
 *   - 底部增加 resetConversation 按钮
 */

import { useCallback, useEffect, useRef, useState } from 'react';
import { Input, Button, Alert, Empty, Typography, Space, Tooltip } from 'antd';
import {
  SendOutlined,
  StopOutlined,
  ReloadOutlined,
} from '@ant-design/icons';
import type { ChatMessage as ChatMessageType } from '@agent/types';
import { ChatMessage } from './ChatMessage';
import { ExamplePicker } from './ExamplePicker';

const { TextArea } = Input;
const { Text } = Typography;

interface ChatPanelProps {
  messages: ChatMessageType[];
  isRunning: boolean;
  error: string | null;
  hasChart: boolean;
  onSend: (text: string) => void;
  onAbort: () => void;
  onClearError: () => void;
  onReset: () => void;
}

export function ChatPanel({
  messages,
  isRunning,
  error,
  hasChart,
  onSend,
  onAbort,
  onClearError,
  onReset,
}: ChatPanelProps) {
  const [input, setInput] = useState('');
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const scrollContainerRef = useRef<HTMLDivElement>(null);
  const userScrolledUpRef = useRef(false);

  useEffect(() => {
    const container = scrollContainerRef.current;
    if (!container) return;
    const handleScroll = () => {
      const { scrollTop, scrollHeight, clientHeight } = container;
      userScrolledUpRef.current = scrollHeight - scrollTop - clientHeight > 80;
    };
    container.addEventListener('scroll', handleScroll);
    return () => container.removeEventListener('scroll', handleScroll);
  }, []);

  useEffect(() => {
    if (!userScrolledUpRef.current) {
      messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
    }
  }, [messages, isRunning]);

  const handleSend = useCallback(() => {
    const text = input.trim();
    if (!text || isRunning) return;
    onSend(text);
    setInput('');
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

  const handlePickExample = useCallback(
    (prompt: string) => {
      if (isRunning) return;
      onSend(prompt);
    },
    [isRunning, onSend],
  );

  const isEmpty = messages.length === 0 && !isRunning;

  return (
    <div className="chat-panel">
      <div className="chat-panel-header">
        <Space style={{ width: '100%', justifyContent: 'space-between' }}>
          <Text strong style={{ fontSize: 14 }}>
            对话区
          </Text>
          {(hasChart || messages.length > 0) && (
            <Tooltip title="清空对话与图表（session 配额仍累加）">
              <Button
                size="small"
                type="text"
                icon={<ReloadOutlined />}
                onClick={onReset}
                disabled={isRunning}
              >
                新对话
              </Button>
            </Tooltip>
          )}
        </Space>
      </div>

      <div ref={scrollContainerRef} className="chat-messages">
        {isEmpty && (
          <div className="chat-empty-state">
            <Empty
              image={Empty.PRESENTED_IMAGE_SIMPLE}
              description={
                <span style={{ fontSize: 13 }}>
                  用自然语言描述你想要的图表
                  <br />
                  或点击下方示例开始
                </span>
              }
              style={{ marginTop: 24 }}
            />
            <ExamplePicker onPick={handlePickExample} disabled={isRunning} hasChart={hasChart} />
          </div>
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
                ? 'Agent 执行中，请等待或点击中止...'
                : '描述你想要的图表，如：画一个用户认证流程图'
            }
            disabled={isRunning}
            autoSize={{ minRows: 1, maxRows: 4 }}
            style={{ resize: 'none' }}
          />
          {isRunning ? (
            <Button danger icon={<StopOutlined />} onClick={onAbort} className="chat-send-btn">
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
            Enter 发送 · Shift+Enter 换行 · Powered by DeepSeek
          </Text>
        </div>
      </div>
    </div>
  );
}
