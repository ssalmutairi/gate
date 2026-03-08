import type { Route, Upstream } from '../../lib/api';

export function UpstreamHint({
  upstreamId,
  routes,
  upstreams,
}: {
  upstreamId: string;
  routes?: Route[];
  upstreams?: Upstream[];
}) {
  if (!upstreamId || !routes || !upstreams) return null;

  const upstream = upstreams.find((u) => u.id === upstreamId);
  const matchingRoutes = routes.filter((r) => r.upstream_id === upstreamId);
  const target = upstream?.targets?.[0];

  if (!upstream && !matchingRoutes.length) return null;

  return (
    <div className="bg-muted/50 border border-border rounded px-3 py-2 text-xs text-muted-foreground space-y-0.5 overflow-hidden">
      {target && (
        <p className="truncate">
          Target: <span className="font-mono">{target.tls ? 'https' : 'http'}://{target.host}:{target.port}</span>
        </p>
      )}
      {matchingRoutes.map((r) => (
        <p key={r.id}>
          Route "<span className="font-medium text-foreground">{r.name}</span>"
          {r.strip_prefix && ' strips prefix'}
          {r.upstream_path_prefix && (
            <> &rarr; upstream path prefix: <span className="font-mono text-foreground">{r.upstream_path_prefix}</span></>
          )}
        </p>
      ))}
      {matchingRoutes.some((r) => r.upstream_path_prefix) && (
        <p className="text-warning">
          Compositions call upstreams directly — use the actual upstream path, not the gateway route prefix.
        </p>
      )}
    </div>
  );
}
