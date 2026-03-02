import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { getLogs } from '../lib/api';
import { Button } from '../components/ui/button';
import { Card } from '../components/ui/card';
import { Badge } from '../components/ui/badge';
import { useTimezone } from '../hooks/useTimezone';
import { formatTime } from '../lib/date';

function statusColor(code: number) {
  if (code >= 500) return 'destructive' as const;
  if (code >= 400) return 'warning' as const;
  return 'success' as const;
}

export default function LogsPage() {
  const [page, setPage] = useState(1);
  const limit = 20;
  const { timezone } = useTimezone();

  const logs = useQuery({
    queryKey: ['logs', page],
    queryFn: () => getLogs({ page, limit }),
    refetchInterval: 10000,
  });

  const totalPages = logs.data ? Math.ceil(logs.data.total / limit) : 0;

  return (
    <div>
      <div className="flex items-center justify-between mb-6">
        <h1 className="text-2xl font-bold">Request Logs</h1>
        <span className="text-sm text-muted-foreground">
          {logs.data?.total ?? 0} total entries
        </span>
      </div>

      <Card>
        <div className="overflow-x-auto">
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b border-border text-left text-muted-foreground">
                <th className="px-4 py-3 font-medium">Time</th>
                <th className="px-4 py-3 font-medium">Method</th>
                <th className="px-4 py-3 font-medium">Path</th>
                <th className="px-4 py-3 font-medium">Status</th>
                <th className="px-4 py-3 font-medium">Latency</th>
                <th className="px-4 py-3 font-medium">Client IP</th>
                <th className="px-4 py-3 font-medium">Upstream</th>
              </tr>
            </thead>
            <tbody>
              {logs.data?.data.map((entry) => (
                <tr key={entry.id} className="border-b border-border last:border-0 hover:bg-muted/50">
                  <td className="px-4 py-3 text-muted-foreground text-xs">
                    {formatTime(entry.created_at, timezone)}
                  </td>
                  <td className="px-4 py-3">
                    <Badge variant="muted">{entry.method}</Badge>
                  </td>
                  <td className="px-4 py-3 font-mono text-xs">{entry.path}</td>
                  <td className="px-4 py-3">
                    <Badge variant={statusColor(entry.status_code)}>
                      {entry.status_code}
                    </Badge>
                  </td>
                  <td className="px-4 py-3 text-muted-foreground">
                    {entry.latency_ms.toFixed(1)}ms
                  </td>
                  <td className="px-4 py-3 font-mono text-xs">{entry.client_ip}</td>
                  <td className="px-4 py-3 text-xs text-muted-foreground">
                    {entry.upstream_target ?? '-'}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>

        {totalPages > 1 && (
          <div className="flex items-center justify-between px-4 py-3 border-t border-border">
            <Button
              variant="secondary"
              size="sm"
              disabled={page <= 1}
              onClick={() => setPage((p) => p - 1)}
            >
              Previous
            </Button>
            <span className="text-sm text-muted-foreground">
              Page {page} of {totalPages}
            </span>
            <Button
              variant="secondary"
              size="sm"
              disabled={page >= totalPages}
              onClick={() => setPage((p) => p + 1)}
            >
              Next
            </Button>
          </div>
        )}
      </Card>
    </div>
  );
}
