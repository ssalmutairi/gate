import { useQuery } from '@tanstack/react-query';
import { getRoutes, getStats } from '../lib/api';
import { Card } from '../components/ui';
import { Activity, AlertTriangle, Clock, Route } from 'lucide-react';

export default function DashboardPage() {
  const routes = useQuery({ queryKey: ['routes'], queryFn: getRoutes });
  const stats = useQuery({
    queryKey: ['stats'],
    queryFn: getStats,
    refetchInterval: 30000,
  });

  const cards = [
    {
      label: 'Requests Today',
      value: stats.data?.total_requests_today ?? '-',
      icon: Activity,
      color: 'text-blue-500',
    },
    {
      label: 'Error Rate',
      value: stats.data ? `${(stats.data.error_rate * 100).toFixed(1)}%` : '-',
      icon: AlertTriangle,
      color: stats.data && stats.data.error_rate > 0.05 ? 'text-red-500' : 'text-green-500',
    },
    {
      label: 'p95 Latency',
      value: stats.data ? `${stats.data.p95_latency_ms.toFixed(0)}ms` : '-',
      icon: Clock,
      color: 'text-yellow-500',
    },
    {
      label: 'Active Routes',
      value: stats.data?.active_routes ?? '-',
      icon: Route,
      color: 'text-purple-500',
    },
  ];

  return (
    <div>
      <h1 className="text-2xl font-bold mb-6">Dashboard</h1>

      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4 mb-8">
        {cards.map((stat) => {
          const Icon = stat.icon;
          return (
            <Card key={stat.label} className="p-4">
              <div className="flex items-center gap-3">
                <div className={`p-2 rounded-md bg-muted ${stat.color}`}>
                  <Icon className="w-5 h-5" />
                </div>
                <div>
                  <p className="text-sm text-muted-foreground">{stat.label}</p>
                  <p className="text-2xl font-bold">{stat.value}</p>
                </div>
              </div>
            </Card>
          );
        })}
      </div>

      <Card className="p-6">
        <h2 className="text-lg font-semibold mb-4">Routes</h2>
        <div className="space-y-3">
          {routes.data?.map((route) => (
            <div
              key={route.id}
              className="flex items-center justify-between py-2 border-b border-border last:border-0"
            >
              <div>
                <span className="font-medium">{route.name}</span>
                <span className="text-muted-foreground ml-2 text-sm">
                  {route.path_prefix}
                </span>
              </div>
              <span
                className={`text-xs px-2 py-0.5 rounded-full ${
                  route.active
                    ? 'bg-success/10 text-success'
                    : 'bg-muted text-muted-foreground'
                }`}
              >
                {route.active ? 'Active' : 'Inactive'}
              </span>
            </div>
          ))}
          {routes.data?.length === 0 && (
            <p className="text-muted-foreground text-sm">No routes configured yet.</p>
          )}
        </div>
      </Card>
    </div>
  );
}
