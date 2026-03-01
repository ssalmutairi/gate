import { Link, useLocation } from 'react-router-dom';
import {
  LayoutDashboard,
  Route,
  Server,
  Key,
  Gauge,
  ScrollText,
  Blocks,
  Menu,
  X,
} from 'lucide-react';
import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { getHealth } from '../lib/api';

const navItems = [
  { path: '/', label: 'Dashboard', icon: LayoutDashboard },
  { path: '/services', label: 'Services', icon: Blocks },
  { path: '/routes', label: 'Routes', icon: Route },
  { path: '/upstreams', label: 'Upstreams', icon: Server },
  { path: '/api-keys', label: 'API Keys', icon: Key },
  { path: '/rate-limits', label: 'Rate Limits', icon: Gauge },
  { path: '/logs', label: 'Logs', icon: ScrollText },
];

export default function Layout({ children }: { children: React.ReactNode }) {
  const location = useLocation();
  const [sidebarOpen, setSidebarOpen] = useState(false);
  const health = useQuery({ queryKey: ['health'], queryFn: getHealth, staleTime: 60_000 });

  return (
    <div className="flex h-screen bg-muted/30">
      {/* Mobile overlay */}
      {sidebarOpen && (
        <div
          className="fixed inset-0 bg-black/50 z-40 lg:hidden"
          onClick={() => setSidebarOpen(false)}
        />
      )}

      {/* Sidebar */}
      <aside
        className={`fixed lg:static inset-y-0 left-0 z-50 w-64 bg-card border-r border-border transform transition-transform lg:translate-x-0 flex flex-col ${
          sidebarOpen ? 'translate-x-0' : '-translate-x-full'
        }`}
      >
        <div className="flex items-center justify-between h-14 px-4 border-b border-border">
          <Link to="/" className="flex items-center gap-2 font-bold text-lg">
            <Server className="w-5 h-5 text-primary" />
            Gate
          </Link>
          <button
            onClick={() => setSidebarOpen(false)}
            className="lg:hidden p-1 hover:bg-muted rounded"
          >
            <X className="w-5 h-5" />
          </button>
        </div>
        <nav className="p-3 space-y-1 flex-1">
          {navItems.map((item) => {
            const Icon = item.icon;
            const active = item.path === '/'
              ? location.pathname === '/'
              : location.pathname.startsWith(item.path);
            return (
              <Link
                key={item.path}
                to={item.path}
                onClick={() => setSidebarOpen(false)}
                className={`flex items-center gap-3 px-3 py-2 rounded-md text-sm font-medium transition-colors ${
                  active
                    ? 'bg-primary text-primary-foreground'
                    : 'text-muted-foreground hover:bg-muted hover:text-foreground'
                }`}
              >
                <Icon className="w-4 h-4" />
                {item.label}
              </Link>
            );
          })}
        </nav>
        {health.data?.version && (
          <div className="px-4 py-3 border-t border-border text-xs text-muted-foreground">
            v{health.data.version}
          </div>
        )}
      </aside>

      {/* Main content */}
      <div className="flex-1 flex flex-col min-w-0">
        <header className="h-14 border-b border-border bg-card flex items-center px-4 gap-3">
          <button
            onClick={() => setSidebarOpen(true)}
            className="lg:hidden p-1 hover:bg-muted rounded"
          >
            <Menu className="w-5 h-5" />
          </button>
          <h1 className="text-sm font-medium text-muted-foreground">
            API Gateway
          </h1>
        </header>
        <main className="flex-1 overflow-auto p-6">{children}</main>
      </div>
    </div>
  );
}
