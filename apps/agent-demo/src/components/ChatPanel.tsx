/**
 * ChatPanel 对话面板
 *
 * 设计：
 *   - 空状态：大渐变 hero 图标 + 主副标题 + 引导文案
 *   - 输入区：玻璃态背景 + 渐变发送按钮
 *   - 顶部状态条：session 信息 + 新对话
 */

import { useCallback, useEffect, useRef, useState } from 'react';
import { Input, Button, Alert, Space, Tooltip } from 'antd';
import {
  SendOutlined,
  StopOutlined,
  ReloadOutlined,
  ThunderboltFilled,
  CommentOutlined,
} from '@ant-design/icons';
import type { ChatMessage as ChatMessageType } from '@agent/types';
import { ChatMessage } from './ChatMessage';
import { ExamplePicker } from './ExamplePicker';

const { TextArea } = Input;

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
        <Space size={8} align="center">
          <span className="chat-panel-header-icon">
            <CommentOutlined />
          </span>
          <span className="chat-panel-header-title">对话区</span>
          {messages.length > 0 && (
            <span className="chat-panel-header-badge">{messages.length}</span>
          )}
        </Space>
        {(hasChart || messages.length > 0) && (
          <Tooltip title="清空对话与图表（session 配额仍累加）">
            <Button
              size="small"
              type="text"
              icon={<ReloadOutlined />}
              onClick={onReset}
              disabled={isRunning}
              className="chat-panel-reset-btn"
            >
              新对话
            </Button>
          </Tooltip>
        )}
      </div>

      <div ref={scrollContainerRef} className="chat-messages">
        {isEmpty && (
          <div className="chat-empty-state">
            <div className="chat-empty-hero">
              <div className="chat-empty-hero-icon">
                <ThunderboltFilled />
              </div>
              <h2 className="chat-empty-hero-title">开始一段对话</h2>
              <p className="chat-empty-hero-subtitle">
                用自然语言描述你想要的图表<br />
                Tautcore Agent 会自动生成、修改、迭代
              </p>
            </div>

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
            className="chat-error-alert"
          />
        )}

        <div ref={messagesEndRef} />
      </div>

      <div className="chat-input-area">
        <div className={`chat-input-wrapper ${isRunning ? 'is-running' : ''}`}>
          <TextArea
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder={
              isRunning
                ? 'Agent 执行中，请等待或点击中止…'
                : '描述你想要的图表，如：画一个用户认证流程图'
            }
            disabled={isRunning}
            autoSize={{ minRows: 1, maxRows: 5 }}
            className="chat-input"
            variant="borderless"
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
            <Tooltip title="发送 (Enter)">
              <Button
                type="primary"
                icon={<SendOutlined />}
                onClick={handleSend}
                disabled={!input.trim()}
                className="chat-send-btn"
              />
            </Tooltip>
          )}
        </div>
        <div className="chat-input-hint">
          <span>Enter 发送</span>
          <span className="chat-input-hint-dot">·</span>
          <span>Shift+Enter 换行</span>
          <span className="chat-input-hint-dot">·</span>
          <span>Powered by DeepSeek V4</span>
        </div>
      </div>
    </div>
  );
}
