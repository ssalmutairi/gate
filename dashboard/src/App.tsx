import { BrowserRouter, Routes, Route } from 'react-router-dom';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import ThemeProvider from './components/ThemeProvider';
import AuthProvider from './components/AuthProvider';
import Layout from './components/Layout';
import DashboardPage from './pages/DashboardPage';
import RoutesPage from './pages/RoutesPage';
import UpstreamsPage from './pages/UpstreamsPage';
import ApiKeysPage from './pages/ApiKeysPage';
import RateLimitsPage from './pages/RateLimitsPage';
import LogsPage from './pages/LogsPage';
import ServicesPage from './pages/ServicesPage';
import ServiceDetailPage from './pages/ServiceDetailPage';
import SettingsPage from './pages/SettingsPage';
import { Toaster } from './components/ui/sonner';

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      refetchOnWindowFocus: false,
      retry: 1,
    },
  },
});

function App() {
  // Admin token is set via the Settings page — no default hardcoded

  return (
    <ThemeProvider>
      <AuthProvider>
        <QueryClientProvider client={queryClient}>
          <BrowserRouter>
            <Layout>
              <Routes>
                <Route path="/" element={<DashboardPage />} />
                <Route path="/services" element={<ServicesPage />} />
                <Route path="/services/:id" element={<ServiceDetailPage />} />
                <Route path="/routes" element={<RoutesPage />} />
                <Route path="/upstreams" element={<UpstreamsPage />} />
                <Route path="/api-keys" element={<ApiKeysPage />} />
                <Route path="/rate-limits" element={<RateLimitsPage />} />
                <Route path="/logs" element={<LogsPage />} />
                <Route path="/settings" element={<SettingsPage />} />
              </Routes>
            </Layout>
            <Toaster />
          </BrowserRouter>
        </QueryClientProvider>
      </AuthProvider>
    </ThemeProvider>
  );
}

export default App;
