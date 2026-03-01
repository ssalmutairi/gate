import { BrowserRouter, Routes, Route } from 'react-router-dom';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import Layout from './components/Layout';
import DashboardPage from './pages/DashboardPage';
import RoutesPage from './pages/RoutesPage';
import UpstreamsPage from './pages/UpstreamsPage';
import ApiKeysPage from './pages/ApiKeysPage';
import RateLimitsPage from './pages/RateLimitsPage';
import LogsPage from './pages/LogsPage';
import { Toaster } from './components/ui';

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      refetchOnWindowFocus: false,
      retry: 1,
    },
  },
});

function App() {
  // Set admin token if not set
  if (!localStorage.getItem('admin_token')) {
    localStorage.setItem('admin_token', 'changeme');
  }

  return (
    <QueryClientProvider client={queryClient}>
      <BrowserRouter>
        <Layout>
          <Routes>
            <Route path="/" element={<DashboardPage />} />
            <Route path="/routes" element={<RoutesPage />} />
            <Route path="/upstreams" element={<UpstreamsPage />} />
            <Route path="/api-keys" element={<ApiKeysPage />} />
            <Route path="/rate-limits" element={<RateLimitsPage />} />
            <Route path="/logs" element={<LogsPage />} />
          </Routes>
        </Layout>
        <Toaster />
      </BrowserRouter>
    </QueryClientProvider>
  );
}

export default App;
