/**
 * ExamplePicker 一键示例选择器
 *
 * 空状态时显示在对话区，让用户零门槛上手。
 * 已有图表时切换显示增量编辑示例（apply_patch 演示）。
 */

import { Typography, Tooltip } from 'antd';
import { EXAMPLE_PROMPTS, INCREMENTAL_EXAMPLES, type ExamplePrompt } from '@lib/examples';

const { Text } = Typography;

interface ExamplePickerProps {
  onPick: (prompt: string) => void;
  disabled: boolean;
  hasChart: boolean;
}

export function ExamplePicker({ onPick, disabled, hasChart }: ExamplePickerProps) {
  const prompts = hasChart ? INCREMENTAL_EXAMPLES : EXAMPLE_PROMPTS;
  const sectionTitle = hasChart ? '试试增量编辑' : '一键示例';

  return (
    <div className="example-picker">
      <Text type="secondary" style={{ fontSize: 12, marginBottom: 8, display: 'block' }}>
        {sectionTitle}
      </Text>
      <div className="example-grid">
        {prompts.map((ex) => (
          <ExampleButton key={ex.title} example={ex} disabled={disabled} onPick={onPick} />
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
  return (
    <Tooltip title={example.hint ?? example.prompt} placement="top">
      <button
        type="button"
        className="example-btn"
        disabled={disabled}
        onClick={() => onPick(example.prompt)}
      >
        <span className="example-btn-icon">{example.icon}</span>
        <span className="example-btn-title">{example.title}</span>
      </button>
    </Tooltip>
  );
}
