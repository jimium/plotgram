/**
 * Plotgram Agent Demo 应用根组件
 *
 * 布局：顶栏 + 左侧预览区（含 ToolCallTrace + DiffSummary）+ 右侧对话区
 * 演示版移除了 DSL Viewer 和 LLM 配置弹窗，开箱即用。
 */

import { useCallback } from 'react';
import { Layout, App as AntdApp } from 'antd';
import { TopBar } from '@components/TopBar';
import { PreviewCanvas } from '@components/PreviewCanvas';
import { ChatPanel } from '@components/ChatPanel';
import { useAgent } from '@hooks/useAgent';
import { useWasm } from '@hooks/useWasm';
import { downloadSvg } from '@lib/exportImage';
import './styles/app.css';

const { Header, Content } = Layout;

function App() {
  const { wasm, ready, error: wasmError, version } = useWasm();
  const agent = useAgent({ wasm, ready });
  const { message } = AntdApp.useApp();

  const handleExport = useCallback(() => {
    if (agent.currentSvg) {
      downloadSvg(agent.currentSvg);
      message.success('已导出 SVG');
    }
  }, [agent.currentSvg, message]);

  return (
    <Layout className="studio-shell">
      <Header className="studio-header">
        <TopBar
          version={version}
          wasmReady={ready}
          wasmError={wasmError}
          isAgentRunning={agent.isRunning}
          canExport={Boolean(agent.currentSvg)}
          onExport={handleExport}
        />
      </Header>

      <Content className="studio-main">
        <div className="studio-preview-pane">
          <PreviewCanvas
            svg={agent.currentSvg}
            source={agent.currentSource}
            ready={ready}
            isAgentRunning={agent.isRunning}
            lastDiff={agent.lastDiff}
            toolCallTrace={agent.toolCallTrace}
          />
        </div>

        <div className="studio-chat-pane">
          <ChatPanel
            messages={agent.messages}
            isRunning={agent.isRunning}
            error={agent.error}
            hasChart={Boolean(agent.currentSvg)}
            onSend={agent.sendMessage}
            onAbort={agent.abort}
            onClearError={agent.clearError}
            onReset={agent.resetConversation}
          />
        </div>
      </Content>
    </Layout>
  );
}

export default App;
