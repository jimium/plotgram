import { useMemo } from 'react';
import { Alert, Empty, Space, Spin, Tag, Typography } from 'antd';
import { lintSource, parseSource, type PlotgramWasm, type DiagnosticErrorJson } from '@lib/wasm';

const { Text, Paragraph } = Typography;

interface RenderStructureViewerProps {
  source: string;
  wasm: PlotgramWasm | null;
  ready: boolean;
}

function renderDiagnostics(title: string, items: DiagnosticErrorJson[]) {
  if (items.length === 0) {
    return null;
  }

  return (
    <div className="structure-diagnostics">
      <Text strong>{title}</Text>
      <div className="structure-diagnostics-list">
        {items.map((item, index) => (
          <Alert
            key={`${title}-${item.code}-${index}`}
            type={item.severity === 'error' ? 'error' : 'warning'}
            showIcon
            message={item.message}
            description={
              <span>
                {item.code} · {item.location.start.line}:{item.location.start.column}
              </span>
            }
          />
        ))}
      </div>
    </div>
  );
}

export function RenderStructureViewer({ source, wasm, ready }: RenderStructureViewerProps) {
  const parseResult = useMemo(() => {
    if (!source || !wasm || !ready) {
      return null;
    }
    return parseSource(wasm, source);
  }, [ready, source, wasm]);

  const lintResult = useMemo(() => {
    if (!source || !wasm || !ready) {
      return null;
    }
    return lintSource(wasm, source, { advice: true });
  }, [ready, source, wasm]);

  if (!source) {
    return (
      <div className="preview-empty">
        <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="还没有 DSL 源码，先让 Agent 生成图表" />
      </div>
    );
  }

  if (!ready || !wasm) {
    return (
      <div className="structure-loading">
        <Spin tip="结构信息加载中..." />
      </div>
    );
  }

  const astJson = parseResult?.diagram
    ? JSON.stringify(parseResult.diagram, null, 2)
    : '// 当前 AST 不可用';

  const violations = lintResult?.report.violations ?? [];
  const advices = lintResult?.report.advices ?? [];

  return (
    <div className="structure-viewer">
      <div className="structure-section">
        <div className="structure-section-header">
          <div>
            <Text strong>AST</Text>
            <Paragraph className="structure-section-desc">
              当前 DSL 解析后的 Diagram JSON。
            </Paragraph>
          </div>
        </div>
        {renderDiagnostics('AST 诊断', parseResult?.errors ?? [])}
        {renderDiagnostics('AST 警告', parseResult?.warnings ?? [])}
        <pre className="structure-code-block">{astJson}</pre>
      </div>

      <div className="structure-section">
        <div className="structure-section-header">
          <div>
            <Text strong>Lint</Text>
            <Paragraph className="structure-section-desc">
              AST 后展示布局 lint 结果，包含违规、建议与自动修复 hint。
            </Paragraph>
          </div>
          <Space size={8} wrap>
            <Tag color={lintResult?.acceptable ? 'success' : 'error'}>
              {lintResult?.acceptable ? '可接受' : '需处理'}
            </Tag>
            <Tag color="processing">{violations.length} violations</Tag>
            <Tag>{advices.length} advices</Tag>
          </Space>
        </div>

        {renderDiagnostics('Lint 错误', lintResult?.errors ?? [])}
        {renderDiagnostics('Lint 警告', lintResult?.warnings ?? [])}

        {violations.length === 0 ? (
          <Alert
            type="success"
            showIcon
            message="当前没有布局违规"
            description="Lint 没有发现需要处理的几何问题。"
          />
        ) : (
          <div className="lint-result-list">
            {violations.map((violation, index) => {
              const relatedAdvices = advices.filter((advice) => advice.violation_index === index);
              return (
                <div key={`${violation.rule}-${index}`} className="lint-result-card">
                  <div className="lint-result-head">
                    <Space size={8} wrap>
                      <Tag color={violation.severity === 'error' ? 'error' : 'warning'}>
                        {violation.severity}
                      </Tag>
                      <Tag>{violation.rule}</Tag>
                      {violation.edge_index != null ? <Tag>edge #{violation.edge_index}</Tag> : null}
                    </Space>
                  </div>

                  <Paragraph className="lint-result-message">{violation.message}</Paragraph>

                  <div className="lint-meta-line">
                    {Array.isArray(violation.entity_ids) && violation.entity_ids.length > 0 ? (
                      <Text type="secondary">entities: {violation.entity_ids.join(', ')}</Text>
                    ) : null}
                    {Array.isArray(violation.group_ids) && violation.group_ids.length > 0 ? (
                      <Text type="secondary">groups: {violation.group_ids.join(', ')}</Text>
                    ) : null}
                    {Array.isArray(violation.related_edge_indices) && violation.related_edge_indices.length > 0 ? (
                      <Text type="secondary">
                        edge_indices: {violation.related_edge_indices.join(', ')}
                      </Text>
                    ) : null}
                  </div>

                  {relatedAdvices.length > 0 ? (
                    <div className="lint-advice-list">
                      {relatedAdvices.map((advice) => (
                        <Alert
                          key={`${violation.rule}-${advice.priority}-${advice.text}`}
                          type="info"
                          showIcon
                          message={`Advice P${advice.priority} · ${advice.confidence}`}
                          description={
                            <div>
                              <div>{advice.text}</div>
                              {advice.fix ? (
                                <Text type="secondary">fix: {advice.fix.action}</Text>
                              ) : null}
                            </div>
                          }
                        />
                      ))}
                    </div>
                  ) : null}
                </div>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}
