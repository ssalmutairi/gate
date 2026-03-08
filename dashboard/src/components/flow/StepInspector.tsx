import { Input } from '../ui/input';
import { Label } from '../ui/label';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '../ui/select';
import { Button } from '../ui/button';
import { Trash2, Zap, Search, ChevronDown } from 'lucide-react';
import { BodyTemplateBuilder } from './BodyTemplateBuilder';
import { ALL_METHODS, ERROR_POLICIES, type StepForm, type ServiceEndpoint } from './flowTypes';
import type { Route, Upstream, Service } from '../../lib/api';
import { useState, useRef, useEffect } from 'react';

interface StepInspectorProps {
  step: StepForm;
  stepIndex: number;
  upstreams: Upstream[];
  routes: Route[];
  services: Service[];
  serviceEndpoints: Map<string, ServiceEndpoint[]>;
  onUpdate: (field: keyof StepForm, value: any) => void;
  onDelete: () => void;
}

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

  const items = [
    ...specEndpoints.map((ep) => ({
      type: 'spec' as const,
      label: `${ep.method} ${ep.path}`,
      detail: ep.summary,
      method: ep.method,
      path: ep.path,
      endpoint: ep,
      route: undefined as Route | undefined,
    })),
    ...(specEndpoints.length ? [] : matchingRoutes).map((r) => ({
      type: 'route' as const,
      label: `${(r.methods?.[0] ?? 'ANY')} ${r.upstream_path_prefix || r.path_prefix}`,
      detail: `Route: ${r.name}`,
      method: r.methods?.[0] ?? 'GET',
      path: r.upstream_path_prefix || r.path_prefix || '/',
      route: r,
      endpoint: undefined as ServiceEndpoint | undefined,
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

  return (
    <div className="relative" ref={ref}>
      <button
        type="button"
        onClick={() => setOpen(!open)}
        className="w-full h-8 flex items-center justify-between px-3 text-xs border border-border rounded-md hover:bg-muted/50 cursor-pointer"
      >
        <span className="flex items-center gap-1.5 text-muted-foreground">
          <Zap className="w-3.5 h-3.5 text-primary" />
          Quick Fill ({items.length} endpoints)
        </span>
        <ChevronDown className={`w-3.5 h-3.5 transition-transform ${open ? 'rotate-180' : ''}`} />
      </button>

      {open && (
        <div className="absolute left-0 right-0 top-9 z-50 bg-card border border-border rounded-md shadow-lg overflow-hidden">
          <div className="flex items-center gap-1.5 px-3 py-2 border-b border-border">
            <Search className="w-3.5 h-3.5 text-muted-foreground shrink-0" />
            <input
              ref={inputRef}
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              placeholder="Search endpoints..."
              className="w-full text-xs bg-transparent outline-none"
            />
          </div>
          <div className="max-h-56 overflow-y-auto">
            {filtered.length === 0 && (
              <p className="px-3 py-2 text-xs text-muted-foreground">No matches</p>
            )}
            {filtered.map((item, i) => (
              <button
                key={`${item.type}-${i}`}
                type="button"
                onClick={() => {
                  if (item.type === 'spec' && item.endpoint) onPickEndpoint(item.endpoint);
                  else if (item.type === 'route' && item.route) onPickRoute(item.route);
                  setOpen(false);
                  setSearch('');
                }}
                className="w-full text-left px-3 py-2 text-xs hover:bg-muted cursor-pointer"
              >
                <div className="flex items-center gap-1.5 font-mono">
                  <span className={`shrink-0 font-semibold ${item.method === 'GET' ? 'text-green-500' : item.method === 'POST' ? 'text-blue-500' : item.method === 'PUT' ? 'text-amber-500' : item.method === 'DELETE' ? 'text-red-500' : 'text-foreground'}`}>
                    {item.method}
                  </span>
                  <span className="truncate">{item.path}</span>
                </div>
                {item.detail && (
                  <div className="text-[10px] text-muted-foreground mt-0.5 truncate">{item.detail}</div>
                )}
              </button>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

export function StepInspector({
  step,
  stepIndex,
  upstreams,
  routes,
  services,
  serviceEndpoints,
  onUpdate,
  onDelete,
}: StepInspectorProps) {
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
  } = step;

  const hasBody = ['POST', 'PUT', 'PATCH'].includes(method);
  const hasService = services?.some((s) => s.upstream_id === upstream_id) ?? false;
  const hasRoute = routes?.some((r) => r.upstream_id === upstream_id && r.active) ?? false;
  const forceProxy = hasService || hasRoute;

  if (forceProxy && !use_internal_route) {
    setTimeout(() => onUpdate('use_internal_route', true), 0);
  }

  const handlePickRoute = (route: Route) => {
    onUpdate('method', route.methods?.[0] ?? 'GET');
    onUpdate('path_template', route.path_prefix || '/');
    onUpdate('use_internal_route', true);
    if (!name) {
      onUpdate('name', route.name.toLowerCase().replace(/[\s-]+/g, '_'));
    }
  };

  const handlePickEndpoint = (ep: ServiceEndpoint) => {
    onUpdate('method', ep.method);
    onUpdate('use_internal_route', true);
    const resolvedPath = (ep.fullPath || ep.path).replace(/\{(\w+)\}/g, '${request.path.$1}');
    onUpdate('path_template', resolvedPath);
    if (!name) {
      const pathName = ep.path.replace(/^\//, '').replace(/[{}\\/]+/g, '_').replace(/_+$/, '');
      onUpdate('name', `${ep.method.toLowerCase()}_${pathName}`.toLowerCase().replace(/[^a-z0-9_]/g, ''));
    }
    if (ep.requestBodyProperties?.length && ['POST', 'PUT', 'PATCH'].includes(ep.method)) {
      const body: Record<string, string> = {};
      for (const prop of ep.requestBodyProperties) {
        body[prop.name] = `\${request.body.${prop.name}}`;
      }
      onUpdate('body_template', JSON.stringify(body, null, 2));
    }
  };

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <h3 className="text-sm font-semibold">Step {stepIndex}: {name || 'Unnamed'}</h3>
        <Button type="button" variant="ghost" size="sm" onClick={onDelete} className="text-destructive h-7">
          <Trash2 className="w-3.5 h-3.5 mr-1" /> Delete
        </Button>
      </div>

      {/* Upstream */}
      <div className="space-y-1.5">
        <Label className="text-xs">Upstream</Label>
        <Select value={upstream_id} onValueChange={(v) => onUpdate('upstream_id', v)}>
          <SelectTrigger className="h-8 text-xs">
            <SelectValue placeholder="Select upstream" />
          </SelectTrigger>
          <SelectContent>
            {(upstreams ?? []).map((u) => (
              <SelectItem key={u.id} value={u.id}>{u.name}</SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>

      {/* Endpoint picker */}
      {upstream_id && (
        <EndpointPicker
          upstreamId={upstream_id}
          routes={routes}
          serviceEndpoints={serviceEndpoints}
          onPickRoute={handlePickRoute}
          onPickEndpoint={handlePickEndpoint}
        />
      )}

      {/* Name */}
      <div className="space-y-1.5">
        <Label className="text-xs">Name</Label>
        <Input
          value={name}
          onChange={(e) => onUpdate('name', e.target.value)}
          placeholder="step_name"
          className="h-8 text-xs"
        />
      </div>

      {/* Method + Path */}
      <div className="grid grid-cols-[90px_1fr] gap-2">
        <div className="space-y-1.5">
          <Label className="text-xs">Method</Label>
          <Select value={method} onValueChange={(v) => onUpdate('method', v)}>
            <SelectTrigger className="h-8 text-xs">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {ALL_METHODS.map((m) => (
                <SelectItem key={m} value={m}>{m}</SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
        <div className="space-y-1.5">
          <Label className="text-xs">Path</Label>
          <Input
            value={path_template}
            onChange={(e) => onUpdate('path_template', e.target.value)}
            placeholder="/users/${request.path.id}"
            className="h-8 text-xs font-mono"
          />
        </div>
      </div>

      {/* Error + Timeout */}
      <div className="grid grid-cols-2 gap-2">
        <div className="space-y-1.5">
          <Label className="text-xs">On Error</Label>
          <Select value={on_error} onValueChange={(v) => onUpdate('on_error', v)}>
            <SelectTrigger className="h-8 text-xs">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {ERROR_POLICIES.map((p) => (
                <SelectItem key={p} value={p}>{p}</SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
        <div className="space-y-1.5">
          <Label className="text-xs">Timeout (ms)</Label>
          <Input
            type="number"
            value={timeout_ms}
            onChange={(e) => onUpdate('timeout_ms', parseInt(e.target.value) || 10000)}
            className="h-8 text-xs"
          />
        </div>
      </div>

      {/* Proxy routing */}
      {forceProxy ? (
        <span className="text-xs text-muted-foreground">Routed through proxy ({hasService ? 'service' : 'route'})</span>
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

      {/* Default value */}
      {on_error === 'default' && (
        <div className="space-y-1.5">
          <Label className="text-xs">Default Value (JSON)</Label>
          <Input
            value={default_value}
            onChange={(e) => onUpdate('default_value', e.target.value)}
            placeholder="null"
            className="h-8 text-xs font-mono"
          />
        </div>
      )}

      {/* Body template */}
      {hasBody && (
        <BodyTemplateBuilder
          value={body_template}
          onChange={(v) => onUpdate('body_template', v)}
        />
      )}
    </div>
  );
}
