import { useState } from 'react';
import { useParams, Link } from 'react-router-dom';
import { useQuery } from '@tanstack/react-query';
import { getService, getServiceSpec } from '../lib/api';
import { Card, Badge } from '../components/ui';
import { ArrowLeft, ChevronDown, ChevronRight, AlertCircle } from 'lucide-react';

const STATUS_COLORS: Record<string, string> = {
  alpha: 'bg-purple-100 text-purple-800 dark:bg-purple-900 dark:text-purple-200',
  beta: 'bg-blue-100 text-blue-800 dark:bg-blue-900 dark:text-blue-200',
  stable: 'bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-200',
  deprecated: 'bg-red-100 text-red-800 dark:bg-red-900 dark:text-red-200',
};

const METHOD_COLORS: Record<string, string> = {
  get: 'bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-200',
  post: 'bg-blue-100 text-blue-800 dark:bg-blue-900 dark:text-blue-200',
  put: 'bg-amber-100 text-amber-800 dark:bg-amber-900 dark:text-amber-200',
  delete: 'bg-red-100 text-red-800 dark:bg-red-900 dark:text-red-200',
  patch: 'bg-purple-100 text-purple-800 dark:bg-purple-900 dark:text-purple-200',
  options: 'bg-gray-100 text-gray-800 dark:bg-gray-700 dark:text-gray-200',
  head: 'bg-gray-100 text-gray-800 dark:bg-gray-700 dark:text-gray-200',
};

interface Endpoint {
  method: string;
  path: string;
  summary: string;
  description: string;
  tags: string[];
  parameters: { name: string; in: string; type: string; required: boolean; description: string }[];
  responses: { status: string; description: string }[];
}

function parseEndpoints(spec: any): Endpoint[] {
  if (!spec?.paths) return [];
  const endpoints: Endpoint[] = [];
  const methods = ['get', 'post', 'put', 'delete', 'patch', 'options', 'head'];

  for (const [path, pathItem] of Object.entries(spec.paths as Record<string, any>)) {
    for (const method of methods) {
      const op = pathItem?.[method];
      if (!op) continue;

      const parameters = (op.parameters || pathItem.parameters || []).map((p: any) => ({
        name: p.name || '',
        in: p.in || '',
        type: p.schema?.type || p.type || '',
        required: !!p.required,
        description: p.description || '',
      }));

      const responses = Object.entries(op.responses || {}).map(([status, resp]: [string, any]) => ({
        status,
        description: resp.description || '',
      }));

      endpoints.push({
        method,
        path,
        summary: op.summary || '',
        description: op.description || '',
        tags: op.tags || ['Untagged'],
        parameters,
        responses,
      });
    }
  }
  return endpoints;
}

function groupByTag(endpoints: Endpoint[]): Record<string, Endpoint[]> {
  const groups: Record<string, Endpoint[]> = {};
  for (const ep of endpoints) {
    for (const tag of ep.tags.length ? ep.tags : ['Untagged']) {
      if (!groups[tag]) groups[tag] = [];
      groups[tag].push(ep);
    }
  }
  return groups;
}

function countByMethod(endpoints: Endpoint[]): Record<string, number> {
  const counts: Record<string, number> = {};
  for (const ep of endpoints) {
    counts[ep.method] = (counts[ep.method] || 0) + 1;
  }
  return counts;
}

function MethodBadge({ method }: { method: string }) {
  return (
    <span
      className={`inline-block px-2 py-0.5 rounded text-xs font-bold uppercase min-w-[4rem] text-center ${
        METHOD_COLORS[method] || 'bg-gray-100 text-gray-800'
      }`}
    >
      {method}
    </span>
  );
}

function EndpointRow({ endpoint }: { endpoint: Endpoint }) {
  const [expanded, setExpanded] = useState(false);
  const hasDetails = endpoint.description || endpoint.parameters.length > 0 || endpoint.responses.length > 0;

  return (
    <div className="border-b border-border last:border-0">
      <button
        className="w-full flex items-center gap-3 px-4 py-3 hover:bg-muted/50 text-left cursor-pointer"
        onClick={() => hasDetails && setExpanded(!expanded)}
      >
        {hasDetails ? (
          expanded ? <ChevronDown className="w-4 h-4 text-muted-foreground shrink-0" /> : <ChevronRight className="w-4 h-4 text-muted-foreground shrink-0" />
        ) : (
          <span className="w-4 shrink-0" />
        )}
        <MethodBadge method={endpoint.method} />
        <code className="text-sm font-mono">{endpoint.path}</code>
        {endpoint.summary && (
          <span className="text-sm text-muted-foreground ml-auto truncate max-w-[40%]">
            {endpoint.summary}
          </span>
        )}
      </button>

      {expanded && (
        <div className="px-4 pb-4 pl-12 space-y-3">
          {endpoint.description && (
            <p className="text-sm text-muted-foreground">{endpoint.description}</p>
          )}

          {endpoint.parameters.length > 0 && (
            <div>
              <h4 className="text-xs font-semibold uppercase text-muted-foreground mb-1">Parameters</h4>
              <div className="overflow-x-auto">
                <table className="w-full text-xs">
                  <thead>
                    <tr className="border-b border-border">
                      <th className="text-left py-1 pr-4 font-medium text-muted-foreground">Name</th>
                      <th className="text-left py-1 pr-4 font-medium text-muted-foreground">In</th>
                      <th className="text-left py-1 pr-4 font-medium text-muted-foreground">Type</th>
                      <th className="text-left py-1 pr-4 font-medium text-muted-foreground">Required</th>
                      <th className="text-left py-1 font-medium text-muted-foreground">Description</th>
                    </tr>
                  </thead>
                  <tbody>
                    {endpoint.parameters.map((p, i) => (
                      <tr key={i} className="border-b border-border last:border-0">
                        <td className="py-1 pr-4 font-mono">{p.name}</td>
                        <td className="py-1 pr-4">{p.in}</td>
                        <td className="py-1 pr-4">{p.type}</td>
                        <td className="py-1 pr-4">{p.required ? 'Yes' : 'No'}</td>
                        <td className="py-1 text-muted-foreground">{p.description}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </div>
          )}

          {endpoint.responses.length > 0 && (
            <div>
              <h4 className="text-xs font-semibold uppercase text-muted-foreground mb-1">Responses</h4>
              <div className="space-y-1">
                {endpoint.responses.map((r, i) => (
                  <div key={i} className="flex items-center gap-2 text-xs">
                    <span className={`font-mono font-bold ${
                      r.status.startsWith('2') ? 'text-green-600 dark:text-green-400' :
                      r.status.startsWith('4') ? 'text-amber-600 dark:text-amber-400' :
                      r.status.startsWith('5') ? 'text-red-600 dark:text-red-400' :
                      'text-muted-foreground'
                    }`}>{r.status}</span>
                    <span className="text-muted-foreground">{r.description}</span>
                  </div>
                ))}
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

export default function ServiceDetailPage() {
  const { id } = useParams<{ id: string }>();
  const [collapsedTags, setCollapsedTags] = useState<Set<string>>(new Set());

  const serviceQuery = useQuery({
    queryKey: ['service', id],
    queryFn: () => getService(id!),
    enabled: !!id,
  });

  const specQuery = useQuery({
    queryKey: ['serviceSpec', id],
    queryFn: () => getServiceSpec(id!),
    enabled: !!id,
  });

  const service = serviceQuery.data;
  const spec = specQuery.data;
  const endpoints = spec ? parseEndpoints(spec) : [];
  const grouped = groupByTag(endpoints);
  const methodCounts = countByMethod(endpoints);
  const tagNames = Object.keys(grouped).sort();

  const toggleTag = (tag: string) => {
    setCollapsedTags((prev) => {
      const next = new Set(prev);
      if (next.has(tag)) next.delete(tag);
      else next.add(tag);
      return next;
    });
  };

  if (serviceQuery.isLoading) {
    return <div className="p-8 text-center text-muted-foreground">Loading...</div>;
  }

  if (serviceQuery.isError || !service) {
    return (
      <div className="p-8 text-center">
        <p className="text-destructive">Failed to load service</p>
        <Link to="/services" className="text-sm text-primary hover:underline mt-2 inline-block">
          Back to Services
        </Link>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center gap-4">
        <Link
          to="/services"
          className="p-2 hover:bg-muted rounded-md transition-colors"
        >
          <ArrowLeft className="w-5 h-5" />
        </Link>
        <div className="flex-1">
          <div className="flex items-center gap-3">
            <h2 className="text-2xl font-bold">/{service.namespace}</h2>
            <Badge>v{service.version}</Badge>
            <span className={`inline-block px-2 py-0.5 rounded-full text-xs font-medium ${STATUS_COLORS[service.status] || ''}`}>
              {service.status}
            </span>
          </div>
          {service.description && (
            <p className="text-sm text-muted-foreground mt-1">{service.description}</p>
          )}
        </div>
      </div>

      {/* API Info from spec */}
      {spec?.info && (
        <Card className="p-4">
          <div className="flex items-start justify-between">
            <div>
              <h3 className="font-semibold">{spec.info.title || 'API'}</h3>
              {spec.info.version && (
                <span className="text-xs text-muted-foreground">Spec version: {spec.info.version}</span>
              )}
              {spec.info.description && (
                <p className="text-sm text-muted-foreground mt-1 max-w-2xl">{spec.info.description}</p>
              )}
            </div>
          </div>
        </Card>
      )}

      {/* Endpoint stats */}
      {endpoints.length > 0 && (
        <div className="flex gap-2 flex-wrap">
          <span className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-md bg-muted text-sm font-medium">
            {endpoints.length} endpoint{endpoints.length !== 1 ? 's' : ''}
          </span>
          {Object.entries(methodCounts).sort().map(([method, count]) => (
            <span key={method} className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-md text-sm">
              <MethodBadge method={method} />
              <span className="text-muted-foreground">{count}</span>
            </span>
          ))}
        </div>
      )}

      {/* Endpoints grouped by tag */}
      {specQuery.isLoading ? (
        <div className="p-8 text-center text-muted-foreground">Loading spec...</div>
      ) : !spec ? (
        <Card className="p-8">
          <div className="flex flex-col items-center text-center gap-2">
            <AlertCircle className="w-8 h-8 text-muted-foreground" />
            <p className="text-muted-foreground">No spec data stored for this service.</p>
            <p className="text-xs text-muted-foreground">Re-import the service to view its API endpoints.</p>
          </div>
        </Card>
      ) : endpoints.length === 0 ? (
        <Card className="p-8 text-center text-muted-foreground">
          No endpoints found in spec.
        </Card>
      ) : (
        <div className="space-y-4">
          {tagNames.map((tag) => {
            const isCollapsed = collapsedTags.has(tag);
            const tagEndpoints = grouped[tag];
            return (
              <Card key={tag}>
                <button
                  className="w-full flex items-center gap-2 px-4 py-3 font-semibold text-sm hover:bg-muted/50 cursor-pointer"
                  onClick={() => toggleTag(tag)}
                >
                  {isCollapsed ? (
                    <ChevronRight className="w-4 h-4 text-muted-foreground" />
                  ) : (
                    <ChevronDown className="w-4 h-4 text-muted-foreground" />
                  )}
                  {tag}
                  <span className="text-xs text-muted-foreground font-normal ml-1">
                    ({tagEndpoints.length})
                  </span>
                </button>
                {!isCollapsed && (
                  <div className="border-t border-border">
                    {tagEndpoints.map((ep, i) => (
                      <EndpointRow key={`${ep.method}-${ep.path}-${i}`} endpoint={ep} />
                    ))}
                  </div>
                )}
              </Card>
            );
          })}
        </div>
      )}
    </div>
  );
}
