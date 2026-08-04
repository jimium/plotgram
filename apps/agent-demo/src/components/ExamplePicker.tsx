/**
 * ExamplePicker 一键示例选择器
 *
 * 设计：6 张示例卡片，hover 浮起 + 阴影变化；
 * 卡片左侧有彩色图标背景块，右侧标题+短描述。
 */

import { Tooltip } from 'antd';
import { ThunderboltFilled } from '@ant-design/icons';
import { EXAMPLE_PROMPTS, INCREMENTAL_EXAMPLES, type ExamplePrompt } from '@lib/examples';

interface ExamplePickerProps {
  onPick: (prompt: string) => void;
  disabled: boolean;
  hasChart: boolean;
}

const ICON_BG: Record<string, string> = {
  电商下单流程: 'linear-gradient(135deg, #f97316 0%, #ec4899 100%)',
  微服务架构图: 'linear-gradient(135deg, #3b82f6 0%, #06b6d4 100%)',
  用户认证时序图: 'linear-gradient(135deg, #8b5cf6 0%, #d946ef 100%)',
  订单状态机: 'linear-gradient(135deg, #10b981 0%, #14b8a6 100%)',
  博客ER图: 'linear-gradient(135deg, #f59e0b 0%, #ef4444 100%)',
  AI学习路线: 'linear-gradient(135deg, #6366f1 0%, #8b5cf6 100%)',
  给架构图加缓存层: 'linear-gradient(135deg, #ec4899 0%, #f43f5e 100%)',
  换横向布局: 'linear-gradient(135deg, #14b8a6 0%, #06b6d4 100%)',
};

export function ExamplePicker({ onPick, disabled, hasChart }: ExamplePickerProps) {
  const prompts = hasChart ? INCREMENTAL_EXAMPLES : EXAMPLE_PROMPTS;
  const sectionTitle = hasChart ? '试试增量编辑' : '一键示例';
  const sectionSub = hasChart
    ? '在已有图表上做精准修改（apply_patch）'
    : '点击直接体验 · 覆盖 6 种主流图表';

  return (
    <div className="example-picker">
      <div className="example-picker-header">
        <div className="example-picker-title">
          <ThunderboltFilled className="example-picker-title-icon" />
          {sectionTitle}
        </div>
        <div className="example-picker-subtitle">{sectionSub}</div>
      </div>
      <div className="example-grid">
        {prompts.map((ex) => (
          <ExampleButton
            key={ex.title}
            example={ex}
            disabled={disabled}
            onPick={onPick}
          />
        ))}
      </div>
    </div>
  );
}

function ExampleButton({
  example,
  disabled,
  onPick,
}: {
  example: ExamplePrompt;
  disabled: boolean;
  onPick: (prompt: string) => void;
}) {
  const bg = ICON_BG[example.title] ?? 'linear-gradient(135deg, #7c3aed 0%, #a855f7 100%)';
  return (
    <Tooltip title={example.hint ?? example.prompt} placement="top">
      <button
        type="button"
        className="example-btn"
        disabled={disabled}
        onClick={() => onPick(example.prompt)}
      >
        <span className="example-btn-icon" style={{ background: bg }}>
          {example.icon}
        </span>
        <span className="example-btn-content">
          <span className="example-btn-title">{example.title}</span>
          {example.hint && <span className="example-btn-hint">{example.hint}</span>}
        </span>
      </button>
    </Tooltip>
  );
}
