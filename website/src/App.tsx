import { BrowserRouter, Routes, Route } from 'react-router-dom';
import Layout from './components/Layout';
import Home from './pages/Home';
import GettingStarted from './pages/GettingStarted';
import AgentGuide from './pages/AgentGuide';
import HowItWorks from './pages/HowItWorks';
import TraeStory from './pages/TraeStory';
import Faq from './pages/Faq';

export default function App() {
  return (
    <BrowserRouter>
      <Routes>
        <Route path="/" element={<Layout />}>
          <Route index element={<Home />} />
          <Route path="docs/getting-started" element={<GettingStarted />} />
          <Route path="docs/agent-guide" element={<AgentGuide />} />
          <Route path="docs/how-it-works" element={<HowItWorks />} />
          <Route path="docs/trae-story" element={<TraeStory />} />
          <Route path="docs/faq" element={<Faq />} />
        </Route>
      </Routes>
    </BrowserRouter>
  );
}
