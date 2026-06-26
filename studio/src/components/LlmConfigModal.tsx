/**
 * LLM 配置弹窗
 *
 * 用户在此配置 LLM Provider、API Key、模型等
 * 配置仅存 localStorage,不上传
 */

import { useEffect, useState } from 'react';
import { Modal, Form, Input, Select, InputNumber, Slider, Typography, Alert } from 'antd';
import type { LLMConfig, LLMProvider } from '@lib/llm';
import { saveLLMConfig } from '@lib/configStorage';

const { Text } = Typography;

interface LlmConfigModalProps {
  open: boolean;
  config: LLMConfig;
  onClose: () => void;
  onSave: (config: LLMConfig) => void;
}

const PROVIDER_OPTIONS: Array<{ label: string; value: LLMProvider; baseUrl: string; model: string }> = [
  { label: 'OpenAI', value: 'openai', baseUrl: 'https://api.openai.com/v1', model: 'gpt-4o' },
  { label: 'Anthropic', value: 'anthropic', baseUrl: 'https://api.anthropic.com/v1', model: 'claude-3-5-sonnet-20241022' },
  { label: 'DeepSeek', value: 'deepseek', baseUrl: 'https://api.deepseek.com/v1', model: 'deepseek-chat' },
  { label: 'Ollama (本地)', value: 'ollama', baseUrl: 'http://localhost:11434/v1', model: 'llama3' },
  { label: '自定义 (OpenAI 兼容)', value: 'custom', baseUrl: '', model: '' },
];

export function LlmConfigModal({ open, config, onClose, onSave }: LlmConfigModalProps) {
  const [form] = Form.useForm<LLMConfig>();
  const [provider, setProvider] = useState<LLMProvider>(config.provider);

  useEffect(() => {
    if (open) {
      form.setFieldsValue(config);
      setProvider(config.provider);
    }
  }, [open, config, form]);

  const handleProviderChange = (value: LLMProvider) => {
    setProvider(value);
    const option = PROVIDER_OPTIONS.find((o) => o.value === value);
    if (option) {
      if (option.baseUrl) {
        form.setFieldValue('baseUrl', option.baseUrl);
      }
      if (option.model) {
        form.setFieldValue('model', option.model);
      }
    }
  };

  const handleOk = async () => {
    const values = await form.validateFields();
    saveLLMConfig(values);
    onSave(values);
    onClose();
  };

  return (
    <Modal
      title="LLM 配置"
      open={open}
      onOk={handleOk}
      onCancel={onClose}
      okText="保存"
      cancelText="取消"
      destroyOnHidden
      width={520}
    >
      <Form form={form} layout="vertical" initialValues={config}>
        <Form.Item
          name="provider"
          label="Provider"
          rules={[{ required: true }]}
        >
          <Select
            options={PROVIDER_OPTIONS.map((o) => ({ label: o.label, value: o.value }))}
            onChange={handleProviderChange}
          />
        </Form.Item>

        <Form.Item
          name="apiKey"
          label="API Key"
          rules={[{ required: provider !== 'ollama', message: '请输入 API Key' }]}
        >
          <Input.Password placeholder="sk-..." autoComplete="off" />
        </Form.Item>

        <Form.Item
          name="model"
          label="模型"
          rules={[{ required: true }]}
        >
          <Input placeholder="gpt-4o / claude-3-5-sonnet / deepseek-chat / llama3" />
        </Form.Item>

        <Form.Item
          name="baseUrl"
          label="Base URL"
          rules={[{ required: true }]}
        >
          <Input placeholder="https://api.openai.com/v1" />
        </Form.Item>

        <Form.Item name="temperature" label="Temperature">
          <Slider min={0} max={2} step={0.1} />
        </Form.Item>

        <Form.Item name="maxTokens" label="Max Tokens">
          <InputNumber min={256} max={32768} step={256} style={{ width: '100%' }} />
        </Form.Item>
      </Form>

      <Alert
        type="info"
        showIcon
        message="配置仅保存在浏览器本地"
        description={
          <Text type="secondary" style={{ fontSize: 12 }}>
            API Key 等敏感信息存储于 localStorage,不会上传到任何服务器。
            {provider === 'ollama' && ' Ollama 本地部署无需 API Key。'}
          </Text>
        }
        style={{ marginTop: 8 }}
      />
    </Modal>
  );
}
