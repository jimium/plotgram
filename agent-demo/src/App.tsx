/**
 * Plotgram Agent Demo 应用根组件
 *
 * 三栏布局：左侧预览（SVG + DSL）| 中间对话 | 右侧执行轨迹
 */

import { useCallback } from 'react';
import { Layout, App as AntdApp } from 'antd';
import { TopBar } from '@components/TopBar';
import { PreviewCanvas } from '@components/PreviewCanvas';
import { ChatPanel } from '@components/ChatPanel';
import { ToolCallTrace } from '@components/ToolCallTrace';
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
        {/* 左栏：预览画布 */}
        <div className="studio-preview-pane">
          <PreviewCanvas
            svg={agent.currentSvg}
            source={agent.currentSource}
            ready={ready}
            isAgentRunning={agent.isRunning}
            onRerenderTheme={agent.rerenderWithTheme}
            onRenderDrawio={agent.renderDrawio}
          />
        </div>

        {/* 中栏：对话 */}
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

        {/* 右栏：执行轨迹 */}
        <div className="studio-trace-pane">
          <ToolCallTrace
            items={agent.toolCallTrace}
            running={agent.isRunning}
            lastDiff={agent.lastDiff}
          />
        </div>
      </Content>
    </Layout>
  );
}

export default App;
