import { Link, useLocation } from 'react-router-dom';
import {
  LayoutDashboard,
  Route,
  Key,
  Gauge,
  ScrollText,
  Blocks,
  Menu,
  X,
  Server,
  Settings,
  ChevronsLeft,
  ChevronsRight,
} from 'lucide-react';
import { useState } from 'react';

const navItems = [
  { path: '/', label: 'Dashboard', icon: LayoutDashboard },
  { path: '/services', label: 'Services', icon: Blocks },
  { path: '/routes', label: 'Routes', icon: Route },
  { path: '/upstreams', label: 'Upstreams', icon: Server },
  { path: '/api-keys', label: 'API Keys', icon: Key },
  { path: '/rate-limits', label: 'Rate Limits', icon: Gauge },
  { path: '/logs', label: 'Logs', icon: ScrollText },
  { path: '/settings', label: 'Settings', icon: Settings },
];

export default function Layout({ children }: { children: React.ReactNode }) {
  const location = useLocation();
  const [sidebarOpen, setSidebarOpen] = useState(false);
  const [collapsed, setCollapsed] = useState(() =>
    localStorage.getItem('gate-sidebar-collapsed') === 'true'
  );

  const toggleCollapsed = () => {
    setCollapsed((prev) => {
      localStorage.setItem('gate-sidebar-collapsed', String(!prev));
      return !prev;
    });
  };
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
        className={`fixed lg:static inset-y-0 left-0 z-50 bg-card border-r border-border transform transition-all duration-200 lg:translate-x-0 flex flex-col ${
          sidebarOpen ? 'translate-x-0 w-64' : '-translate-x-full w-64'
        } ${collapsed ? 'lg:w-16' : 'lg:w-64'}`}
      >
        <div className={`flex items-center justify-between h-14 border-b border-border ${collapsed ? 'lg:px-0 lg:justify-center' : ''} px-4`}>
          <Link to="/" className={`flex items-center gap-2 font-bold text-lg ${collapsed ? 'lg:justify-center' : ''}`}>
            <img src="/gate-logo.png" alt="Gate" className="h-6 w-auto object-contain shrink-0" />
            <span className={collapsed ? 'lg:hidden' : ''}>Gate</span>
          </Link>
          <button
            onClick={() => setSidebarOpen(false)}
            className="lg:hidden p-1 hover:bg-muted rounded"
          >
            <X className="w-5 h-5" />
          </button>
        </div>
        <nav className={`p-3 space-y-1 flex-1 ${collapsed ? 'lg:p-2' : ''}`}>
          {navItems.map((item) => {
            const Icon = item.icon;
            const active = item.path === '/'
              ? location.pathname === '/'
              : location.pathname.startsWith(item.path);
            return (
              <Link
                key={item.path}
                to={item.path}
                title={collapsed ? item.label : undefined}
                onClick={() => setSidebarOpen(false)}
                className={`flex items-center gap-3 px-3 py-2 rounded-md text-sm font-medium transition-colors ${
                  collapsed ? 'lg:justify-center lg:px-0' : ''
                } ${
                  active
                    ? 'bg-primary text-primary-foreground'
                    : 'text-muted-foreground hover:bg-muted hover:text-foreground'
                }`}
              >
                <Icon className="w-4 h-4 shrink-0" />
                <span className={collapsed ? 'lg:hidden' : ''}>{item.label}</span>
              </Link>
            );
          })}
        </nav>
        <div className={`border-t border-border ${collapsed ? 'lg:px-2 lg:py-2' : 'px-4 py-3'}`}>
          <div className={`hidden lg:flex items-center text-xs text-muted-foreground ${
            collapsed ? 'justify-center' : 'justify-end px-1'
          }`}>
            <button
              onClick={toggleCollapsed}
              className="p-1 hover:text-foreground transition-colors rounded hover:bg-muted"
              title={collapsed ? 'Expand sidebar' : 'Collapse sidebar'}
            >
              {collapsed ? (
                <ChevronsRight className="w-4 h-4" />
              ) : (
                <ChevronsLeft className="w-4 h-4" />
              )}
            </button>
          </div>
        </div>
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
