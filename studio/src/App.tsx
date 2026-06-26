import { useCallback, useEffect, useState } from 'react';
import { Layout, App as AntdApp } from 'antd';
import { TopBar } from '@components/TopBar';
import { PreviewCanvas } from '@components/PreviewCanvas';
import { ChatPanel } from '@components/ChatPanel';
import { DslViewer } from '@components/DslViewer';
import { LlmConfigModal } from '@components/LlmConfigModal';
import { useAgent } from '@hooks/useAgent';
import { useWasm } from '@hooks/useWasm';
import { loadLLMConfig, type LLMConfig } from '@lib/configStorage';
import { downloadSvg } from '@lib/exportImage';
import './styles/app.css';

const { Header, Content } = Layout;

/**
 * Drawify Studio 应用根组件
 *
 * 布局:顶栏 + 左侧预览区 + 右侧对话区 + 底部 DSL 查看栏
 * 不提供手动 DSL 编辑(那是 Playground 的职责),DSL 由 Agent 生成与迭代
 */
function App() {
  const { wasm, ready, error: wasmError, version, capabilities } = useWasm();
  const [dslViewerOpen, setDslViewerOpen] = useState(false);
  const [configModalOpen, setConfigModalOpen] = useState(false);
  const [llmConfig, setLlmConfig] = useState<LLMConfig>(() => loadLLMConfig());

  const agent = useAgent({ wasm, ready, llmConfig });

  const handleExport = useCallback(() => {
    if (agent.currentSvg) {
      downloadSvg(agent.currentSvg);
    }
  }, [agent.currentSvg]);

  const handleSaveConfig = useCallback((config: LLMConfig) => {
    setLlmConfig(config);
  }, []);

  const contextHolder = AntdApp.useApp();

  // WASM 能力缺失时提示
  useEffect(() => {
    if (ready && (!capabilities.diff || !capabilities.applyPatch)) {
      contextHolder.message.warning(
        '当前 WASM 不支持 diff/apply_patch,增量编辑功能将受限。请重新构建 drawify-wasm。',
      );
    }
  }, [ready, capabilities, contextHolder]);

  return (
    <Layout className="studio-shell">
      <Header className="studio-header">
        <TopBar
          version={version}
          wasmReady={ready}
          wasmError={wasmError}
          isAgentRunning={agent.isRunning}
          canExport={Boolean(agent.currentSvg)}
          onToggleDslViewer={() => setDslViewerOpen((v) => !v)}
          onExport={handleExport}
          onOpenConfig={() => setConfigModalOpen(true)}
        />
      </Header>

      <Content className="studio-main">
        <div className="studio-preview-pane">
          <PreviewCanvas
            svg={agent.currentSvg}
            ready={ready}
            isAgentRunning={agent.isRunning}
            lastDiff={agent.lastDiff}
          />
        </div>

        <div className="studio-chat-pane">
          <ChatPanel
            messages={agent.messages}
            isRunning={agent.isRunning}
            error={agent.error}
            onSend={agent.sendMessage}
            onAbort={agent.abort}
            onClearError={agent.clearError}
          />
        </div>
      </Content>

      <DslViewer
        open={dslViewerOpen}
        source={agent.currentSource}
        onClose={() => setDslViewerOpen(false)}
      />

      <LlmConfigModal
        open={configModalOpen}
        config={llmConfig}
        onClose={() => setConfigModalOpen(false)}
        onSave={handleSaveConfig}
      />
    </Layout>
  );
}

export default App;
