import { memo, useState, useRef, useEffect } from 'react';
import { Handle, Position, type NodeProps, type Node } from '@xyflow/react';
import { Input } from '../ui/input';
import { Label } from '../ui/label';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '../ui/select';
import { X, Zap, Search, ChevronDown } from 'lucide-react';
import { BodyTemplateBuilder } from './BodyTemplateBuilder';
import { ALL_METHODS, ERROR_POLICIES, type StepNodeData, type ServiceEndpoint } from './flowTypes';
import type { Route, Upstream } from '../../lib/api';

type StepNodeType = Node<StepNodeData, 'step'>;

interface PickerItem {
  type: 'spec' | 'route';
  label: string;
  detail?: string;
  method: string;
  path: string;
  route?: Route;
  endpoint?: ServiceEndpoint;
}

/**
 * Searchable dropdown for picking endpoints from service specs and gateway routes.
 */
function EndpointPicker({
  upstreamId,
  routes,
  serviceEndpoints,
  onPickRoute,
  onPickEndpoint,
}: {
  upstreamId: string;
  routes: Route[];
  serviceEndpoints: Map<string, ServiceEndpoint[]>;
  onPickRoute: (route: Route) => void;
  onPickEndpoint: (ep: ServiceEndpoint) => void;
}) {
  const [open, setOpen] = useState(false);
  const [search, setSearch] = useState('');
  const ref = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  const matchingRoutes = routes?.filter((r) => r.upstream_id === upstreamId && r.active) ?? [];
  const specEndpoints = serviceEndpoints?.get(upstreamId) ?? [];

  // Build unified list — only show routes if no spec endpoints exist
  const items: PickerItem[] = [
    ...specEndpoints.map((ep) => ({
      type: 'spec' as const,
      label: `${ep.method} ${ep.path}`,
      detail: ep.summary,
      method: ep.method,
      path: ep.path,
      endpoint: ep,
    })),
    ...(specEndpoints.length ? [] : matchingRoutes).map((r) => ({
      type: 'route' as const,
      label: `${(r.methods?.[0] ?? 'ANY')} ${r.upstream_path_prefix || r.path_prefix}`,
      detail: `Route: ${r.name}`,
      method: r.methods?.[0] ?? 'GET',
      path: r.upstream_path_prefix || r.path_prefix || '/',
      route: r,
    })),
  ];

  const filtered = search
    ? items.filter((it) =>
        it.label.toLowerCase().includes(search.toLowerCase()) ||
        it.detail?.toLowerCase().includes(search.toLowerCase())
      )
    : items;

  useEffect(() => {
    if (!open) return;
    const handler = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as HTMLElement)) {
        setOpen(false);
        setSearch('');
      }
    };
    document.addEventListener('mousedown', handler);
    return () => document.removeEventListener('mousedown', handler);
  }, [open]);

  useEffect(() => {
    if (open) inputRef.current?.focus();
  }, [open]);

  if (!items.length) return null;

  const handlePick = (item: PickerItem) => {
    if (item.type === 'spec' && item.endpoint) onPickEndpoint(item.endpoint);
    else if (item.type === 'route' && item.route) onPickRoute(item.route);
    setOpen(false);
    setSearch('');
  };

  return (
    <div className="space-y-1 relative" ref={ref}>
      <button
        type="button"
        onClick={() => setOpen(!open)}
        className="w-full h-7 flex items-center justify-between px-2 text-xs border border-border rounded-md hover:bg-muted/50 cursor-pointer"
      >
        <span className="flex items-center gap-1 text-muted-foreground">
          <Zap className="w-3 h-3 text-primary" />
          Quick Fill ({items.length} endpoints)
        </span>
        <ChevronDown className={`w-3 h-3 transition-transform ${open ? 'rotate-180' : ''}`} />
      </button>

      {open && (
        <div className="absolute left-0 right-0 top-8 z-50 bg-card border border-border rounded-md shadow-lg overflow-hidden">
          <div className="flex items-center gap-1.5 px-2 py-1.5 border-b border-border">
            <Search className="w-3 h-3 text-muted-foreground shrink-0" />
            <input
              ref={inputRef}
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              placeholder="Search endpoints..."
              className="w-full text-xs bg-transparent outline-none"
            />
          </div>
          <div className="max-h-48 overflow-y-auto">
            {filtered.length === 0 && (
              <p className="px-2 py-2 text-xs text-muted-foreground">No matches</p>
            )}
            {filtered.map((item, i) => {
              const tags = [
                item.endpoint?.parameters?.some(p => p.in === 'path') && 'params',
                item.endpoint?.requestBodyProperties && 'body',
              ].filter(Boolean);
              return (
                <button
                  key={`${item.type}-${i}`}
                  type="button"
                  onClick={() => handlePick(item)}
                  className="w-full text-left px-2 py-1.5 text-xs hover:bg-muted cursor-pointer"
                >
                  <div className="flex items-center gap-1.5 font-mono">
                    <span className={`shrink-0 font-semibold ${item.method === 'GET' ? 'text-green-500' : item.method === 'POST' ? 'text-blue-500' : item.method === 'PUT' ? 'text-amber-500' : item.method === 'DELETE' ? 'text-red-500' : 'text-foreground'}`}>
                      {item.method}
                    </span>
                    <span className="truncate">{item.path}</span>
                    {tags.length > 0 && (
                      <span className="ml-auto shrink-0 flex gap-1">
                        {tags.map((t) => (
                          <span key={t} className="text-[9px] px-1 py-px rounded bg-muted text-muted-foreground">{t}</span>
                        ))}
                      </span>
                    )}
                  </div>
                  {item.detail && (
                    <div className="text-[10px] text-muted-foreground mt-0.5 truncate">{item.detail}</div>
                  )}
                </button>
              );
            })}
          </div>
        </div>
      )}
    </div>
  );
}

function StepNodeComponent({ data }: NodeProps<StepNodeType>) {
  const {
    name,
    method,
    upstream_id,
    path_template,
    on_error,
    default_value,
    timeout_ms,
    body_template,
    use_internal_route,
    upstreams,
    routes,
    services,
    serviceEndpoints,
    stepIndex,
    badgeColor,
    onUpdate,
    onDelete,
    onFocusNode,
  } = data;

  const hasBody = ['POST', 'PUT', 'PATCH'].includes(method);
  // Upstreams with services or routes always go through proxy
  const hasService = services?.some((s) => s.upstream_id === upstream_id) ?? false;
  const hasRoute = routes?.some((r) => r.upstream_id === upstream_id && r.active) ?? false;
  const forceProxy = hasService || hasRoute;

  // Auto-enable internal route when upstream has a service or route
  if (forceProxy && !use_internal_route) {
    setTimeout(() => onUpdate('use_internal_route', true), 0);
  }

  const handlePickRoute = (route: Route) => {
    const routeMethod = route.methods?.[0] ?? 'GET';
    onUpdate('method', routeMethod);
    // Use route path_prefix so the request goes through the proxy route
    const path = route.path_prefix || '/';
    onUpdate('path_template', path);
    onUpdate('use_internal_route', true);
    if (!name) {
      const snakeName = route.name.toLowerCase().replace(/[\s-]+/g, '_');
      onUpdate('name', snakeName);
    }
  };

  const handlePickEndpoint = (ep: ServiceEndpoint) => {
    onUpdate('method', ep.method);
    onUpdate('use_internal_route', true);

    // Use fullPath (includes server base path like /api/v3) and convert path params
    const resolvedPath = (ep.fullPath || ep.path).replace(/\{(\w+)\}/g, '${request.path.$1}');
    onUpdate('path_template', resolvedPath);

    if (!name) {
      const pathName = ep.path.replace(/^\//, '').replace(/[{}\/]+/g, '_').replace(/_+$/, '');
      const snakeName = `${ep.method.toLowerCase()}_${pathName}`.toLowerCase().replace(/[^a-z0-9_]/g, '');
      onUpdate('name', snakeName);
    }

    // Auto-generate body_template from request body schema
    if (ep.requestBodyProperties?.length && ['POST', 'PUT', 'PATCH'].includes(ep.method)) {
      const body: Record<string, string> = {};
      for (const prop of ep.requestBodyProperties) {
        body[prop.name] = `\${request.body.${prop.name}}`;
      }
      onUpdate('body_template', JSON.stringify(body, null, 2));
    }
  };

  return (
    <div className="relative bg-card border border-border rounded-lg shadow-sm w-[320px] text-foreground overflow-visible">
      {/* Index badge */}
      <button
        type="button"
        onClick={onFocusNode}
        style={{ backgroundColor: badgeColor }}
        className="absolute -top-3 -left-3 w-6 h-6 rounded-full text-white text-[11px] font-bold flex items-center justify-center shadow cursor-pointer hover:scale-110 transition-transform z-10"
        title="Focus this step"
      >
        {stepIndex}
      </button>
      <Handle type="target" position={Position.Left} className="!bg-primary !w-3 !h-3" />

      <div className="step-node-header flex items-center justify-between px-3 py-2 border-b border-border cursor-grab bg-muted/30 rounded-t-lg">
        <div className="flex items-center gap-1.5 min-w-0">
          <span className="text-sm font-medium truncate">
            {name || 'Unnamed Step'}
          </span>
        </div>
        <button
          type="button"
          onClick={onDelete}
          className="p-0.5 hover:bg-muted rounded text-destructive cursor-pointer shrink-0"
        >
          <X className="w-3 h-3" />
        </button>
      </div>

      <div className="p-3 space-y-2 nodrag">
        {/* Upstream selector — top priority */}
        <div className="space-y-1">
          <Label className="text-xs">Upstream</Label>
          <Select value={upstream_id} onValueChange={(v) => onUpdate('upstream_id', v)}>
            <SelectTrigger className="h-7 text-xs">
              <SelectValue placeholder="Select upstream" />
            </SelectTrigger>
            <SelectContent>
              {(upstreams ?? []).map((u: Upstream) => (
                <SelectItem key={u.id} value={u.id}>{u.name}</SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>

        {/* Endpoint picker — service spec endpoints + route chips */}
        {upstream_id && (
          <EndpointPicker
            upstreamId={upstream_id}
            routes={routes}
            serviceEndpoints={serviceEndpoints}
            onPickRoute={handlePickRoute}
            onPickEndpoint={handlePickEndpoint}
          />
        )}

        <div className="space-y-1">
          <Label className="text-xs">Name</Label>
          <Input
            value={name}
            onChange={(e) => onUpdate('name', e.target.value)}
            placeholder="step_name"
            className="h-7 text-xs"
          />
        </div>

        <div className="grid grid-cols-[80px_1fr] gap-2">
          <div className="space-y-1">
            <Label className="text-xs">Method</Label>
            <Select value={method} onValueChange={(v) => onUpdate('method', v)}>
              <SelectTrigger className="h-7 text-xs">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {ALL_METHODS.map((m) => (
                  <SelectItem key={m} value={m}>{m}</SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
          <div className="space-y-1">
            <Label className="text-xs">Path</Label>
            <Input
              value={path_template}
              onChange={(e) => onUpdate('path_template', e.target.value)}
              placeholder="/users/${request.path.id}"
              className="h-7 text-xs font-mono"
            />
          </div>
        </div>

        <div className="grid grid-cols-2 gap-2">
          <div className="space-y-1">
            <Label className="text-xs">On Error</Label>
            <Select value={on_error} onValueChange={(v) => onUpdate('on_error', v)}>
              <SelectTrigger className="h-7 text-xs">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {ERROR_POLICIES.map((p) => (
                  <SelectItem key={p} value={p}>{p}</SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
          <div className="space-y-1">
            <Label className="text-xs">Timeout (ms)</Label>
            <Input
              type="number"
              value={timeout_ms}
              onChange={(e) => onUpdate('timeout_ms', parseInt(e.target.value) || 10000)}
              className="h-7 text-xs"
            />
          </div>
        </div>

        {forceProxy ? (
          <span className="text-[10px] text-muted-foreground">Routed through proxy ({hasService ? 'service' : 'route'})</span>
        ) : (
          <label className="flex items-center gap-2 text-xs cursor-pointer">
            <input
              type="checkbox"
              checked={use_internal_route ?? false}
              onChange={(e) => onUpdate('use_internal_route', e.target.checked)}
              className="rounded border-border"
            />
            <span className="text-muted-foreground">Route through proxy</span>
          </label>
        )}

        {on_error === 'default' && (
          <div className="space-y-1">
            <Label className="text-xs">Default Value (JSON)</Label>
            <Input
              value={default_value}
              onChange={(e) => onUpdate('default_value', e.target.value)}
              placeholder="null"
              className="h-7 text-xs font-mono"
            />
          </div>
        )}

        {hasBody && (
          <BodyTemplateBuilder
            value={body_template}
            onChange={(v) => onUpdate('body_template', v)}
          />
        )}
      </div>

      <Handle type="source" position={Position.Right} className="!bg-primary !w-3 !h-3" />
    </div>
  );
}

export const StepNode = memo(StepNodeComponent);
