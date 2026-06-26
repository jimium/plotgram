/**
 * Agent Tool 定义与执行器
 *
 * 每个 Tool 对应 drawify-wasm 的一个能力,
 * Agent 通过 function-calling 调用这些 Tool 操控图表
 */

import type {
  ToolSchema,
  ToolExecutor,
  RenderFormat,
  RenderOptions,
} from './types';
import {
  renderSource,
  validateSource,
  parseSource,
  diffSources,
  applyPatch,
  loadWasm,
  type DrawifyWasm,
} from '@lib/wasm';

/** Agent 可用的 Tool Schema 列表(供 LLM function-calling) */
export const AGENT_TOOL_SCHEMAS: ToolSchema[] = [
  {
    type: 'function',
    function: {
      name: 'render',
      description:
        '渲染 Drawify DSL 为指定格式(svg/ascii/json)。返回渲染结果或错误诊断。生成或修改图表后必须调用以获取可视化结果。',
      parameters: {
        type: 'object',
        properties: {
          source: { type: 'string', description: 'Drawify DSL 源码' },
          format: {
            type: 'string',
            enum: ['svg', 'ascii', 'json'],
            description: '输出格式,默认 svg',
          },
          options: {
            type: 'object',
            description: '渲染选项(主题、风格等)',
            properties: {
              theme_id: { type: 'string' },
              graphic_style: { type: 'string' },
              dark_mode: { type: 'boolean' },
            },
          },
        },
        required: ['source', 'format'],
      },
    },
  },
  {
    type: 'function',
    function: {
      name: 'validate',
      description:
        '校验 Drawify DSL 语法和语义,返回错误和警告(含错误码、行号和修复建议)。生成 DSL 后应调用以自检。',
      parameters: {
        type: 'object',
        properties: {
          source: { type: 'string', description: 'Drawify DSL 源码' },
        },
        required: ['source'],
      },
    },
  },
  {
    type: 'function',
    function: {
      name: 'parse',
      description:
        '解析 Drawify DSL 为 AST JSON,获取实体/关系/分组的结构化信息。用于理解当前图表结构。',
      parameters: {
        type: 'object',
        properties: {
          source: { type: 'string', description: 'Drawify DSL 源码' },
        },
        required: ['source'],
      },
    },
  },
  {
    type: 'function',
    function: {
      name: 'diff',
      description:
        '比较两份 Drawify DSL 的语义差异,返回结构化变更列表(新增/删除/修改的实体、关系、分组)。用于向用户展示变更摘要。',
      parameters: {
        type: 'object',
        properties: {
          old_source: { type: 'string', description: '修改前的 DSL 源码' },
          new_source: { type: 'string', description: '修改后的 DSL 源码' },
        },
        required: ['old_source', 'new_source'],
      },
    },
  },
  {
    type: 'function',
    function: {
      name: 'apply_patch',
      description:
        '对当前 DSL 应用增量补丁(Change 列表),返回修改后的完整 DSL。用于精确修改而不重写整个文件。修改实体属性、增删关系时优先使用。',
      parameters: {
        type: 'object',
        properties: {
          source: { type: 'string', description: '当前 DSL 源码' },
          patch: {
            type: 'array',
            description: '变更列表',
            items: {
              type: 'object',
              properties: {
                op: { type: 'string', enum: ['add', 'remove', 'modify'] },
                path: {
                  type: 'object',
                  properties: {
                    target: {
                      type: 'string',
                      enum: ['entity', 'relation', 'group', 'attribute'],
                    },
                    id: { type: 'string' },
                    attr_key: { type: 'string' },
                  },
                },
                new_value: { type: 'object' },
                old_value: { type: 'object' },
              },
            },
          },
        },
        required: ['source', 'patch'],
      },
    },
  },
  {
    type: 'function',
    function: {
      name: 'layout_catalog',
      description:
        '查询可用的布局算法和边路由算法,以及每种图表类型的默认配置。需要选择布局算法时调用。',
      parameters: { type: 'object', properties: {} },
    },
  },
];

/** 创建 Tool 执行器映射 */
export function createToolExecutors(
  getWasm: () => Promise<DrawifyWasm>,
): Record<string, ToolExecutor> {
  return {
    render: async (args, context) => {
      const wasm = await getWasm();
      const source = String(args.source ?? '');
      const format = String(args.format ?? 'svg') as RenderFormat;
      const options = args.options as RenderOptions | undefined;
      const optionsJson = options ? JSON.stringify(options) : undefined;
      // LLM 通过 render 传入新生成的 DSL,同步到 context 以便 DslViewer 展示
      if (source && source !== context.source) {
        context.source = source;
      }
      return renderSource(wasm, source, format, optionsJson);
    },

    validate: async (args) => {
      const wasm = await getWasm();
      const source = String(args.source ?? '');
      return validateSource(wasm, source);
    },

    parse: async (args) => {
      const wasm = await getWasm();
      const source = String(args.source ?? '');
      return parseSource(wasm, source);
    },

    diff: async (args) => {
      const wasm = await getWasm();
      const oldSource = String(args.old_source ?? '');
      const newSource = String(args.new_source ?? '');
      return diffSources(wasm, oldSource, newSource);
    },

    apply_patch: async (args, context) => {
      const wasm = await getWasm();
      const source = String(args.source ?? context.source);
      const patch = args.patch as unknown[];
      return applyPatch(wasm, source, patch);
    },

    layout_catalog: async () => {
      const wasm = await getWasm();
      return JSON.parse(wasm.layout_catalog());
    },
  };
}

/** 默认 Tool 执行器(使用全局 WASM 单例) */
export const defaultToolExecutors = createToolExecutors(loadWasm);
