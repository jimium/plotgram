/**
 * LLM 配置本地存储
 *
 * API Key 等敏感信息仅存 localStorage,不上传任何服务器
 */

import type { LLMConfig, LLMProvider } from './llm';

export type { LLMConfig, LLMProvider };

const STORAGE_KEY = 'drawify-studio.llm-config';

const DEFAULT_CONFIG: LLMConfig = {
  provider: 'openai',
  apiKey: '',
  model: 'gpt-4o',
  baseUrl: 'https://api.openai.com/v1',
  maxTokens: 4096,
  temperature: 0.7,
};

/** 从 localStorage 读取配置,合并环境变量默认值 */
export function loadLLMConfig(): LLMConfig {
  const stored = localStorage.getItem(STORAGE_KEY);
  const envConfig = loadFromEnv();

  if (!stored) {
    return envConfig;
  }

  try {
    const parsed = JSON.parse(stored) as Partial<LLMConfig>;
    return { ...envConfig, ...parsed };
  } catch {
    return envConfig;
  }
}

/** 保存配置到 localStorage */
export function saveLLMConfig(config: LLMConfig): void {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(config));
}

/** 从环境变量读取默认配置 */
function loadFromEnv(): LLMConfig {
  return {
    provider: (import.meta.env.VITE_LLM_PROVIDER ?? 'openai') as LLMProvider,
    apiKey: import.meta.env.VITE_LLM_API_KEY ?? '',
    model: import.meta.env.VITE_LLM_MODEL ?? 'gpt-4o',
    baseUrl: import.meta.env.VITE_LLM_BASE_URL ?? 'https://api.openai.com/v1',
    maxTokens: 4096,
    temperature: 0.7,
  };
}

export { DEFAULT_CONFIG };
