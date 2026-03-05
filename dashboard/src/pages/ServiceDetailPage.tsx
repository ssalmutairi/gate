import { useState, useMemo } from 'react';
import { useParams, Link } from 'react-router-dom';
import { useQuery } from '@tanstack/react-query';
import { getService, getServiceSpec } from '../lib/api';
import { extractRequestBody, extractResponseSchemas, generateExample } from '../lib/openapi';
import { Card } from '../components/ui/card';
import { Badge } from '../components/ui/badge';
import { ArrowLeft, ChevronDown, ChevronRight, AlertCircle, Play, Loader2 } from 'lucide-react';

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
  operationRef: any;
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
        operationRef: op,
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

// --- SchemaView: Recursive schema property tree ---

function SchemaView({ schema, depth = 0 }: { schema: any; depth?: number }) {
  if (!schema || depth > 6) {
    return <span className="text-xs text-muted-foreground italic">...</span>;
  }

  if (schema._variants) {
    return (
      <div className="pl-3 border-l border-border ml-1 space-y-1">
        <span className="text-xs text-muted-foreground italic">one of:</span>
        {schema._variants.map((v: any, i: number) => (
          <div key={i} className="pl-2">
            <SchemaView schema={v} depth={depth + 1} />
          </div>
        ))}
      </div>
    );
  }

  const type = schema.type;

  // Array
  if (type === 'array' && schema.items) {
    return (
      <div>
        <span className="text-xs text-muted-foreground">array of:</span>
        <div className="pl-3 border-l border-border ml-1">
          <SchemaView schema={schema.items} depth={depth + 1} />
        </div>
      </div>
    );
  }

  // Object
  if (type === 'object' || schema.properties) {
    const required = new Set(schema.required || []);
    const props = schema.properties || {};
    const entries = Object.entries(props);
    if (entries.length === 0 && schema.additionalProperties) {
      return (
        <div>
          <span className="text-xs text-muted-foreground">{'object { [key: string]: '}</span>
          <SchemaView schema={schema.additionalProperties} depth={depth + 1} />
          <span className="text-xs text-muted-foreground">{' }'}</span>
        </div>
      );
    }
    return (
      <div className="space-y-0.5">
        {entries.map(([name, prop]: [string, any]) => (
          <div key={name} className="flex items-start gap-2 pl-3 border-l border-border ml-1 py-0.5">
            <span className="text-xs font-semibold font-mono shrink-0">{name}</span>
            <span className="text-xs text-muted-foreground shrink-0">
              {prop.type || (prop.properties ? 'object' : prop.items ? 'array' : '')}
            </span>
            {required.has(name) && (
              <span className="text-[10px] px-1 py-0 rounded bg-red-100 text-red-700 dark:bg-red-900 dark:text-red-300 shrink-0">
                required
              </span>
            )}
            {prop.enum && (
              <span className="text-[10px] text-muted-foreground shrink-0">
                [{prop.enum.join(', ')}]
              </span>
            )}
            {prop.description && (
              <span className="text-[10px] text-muted-foreground truncate">{prop.description}</span>
            )}
            {(prop.properties || prop.items || prop._variants) && (
              <div className="w-full">
                <SchemaView schema={prop} depth={depth + 1} />
              </div>
            )}
          </div>
        ))}
        {schema.additionalProperties && typeof schema.additionalProperties === 'object' && (
          <div className="pl-3 border-l border-border ml-1 py-0.5">
            <span className="text-xs text-muted-foreground italic">additional properties: </span>
            <SchemaView schema={schema.additionalProperties} depth={depth + 1} />
          </div>
        )}
      </div>
    );
  }

  // Primitive
  return (
    <span className="text-xs text-muted-foreground">
      {type || 'any'}
      {schema.format ? ` (${schema.format})` : ''}
      {schema.enum ? ` [${schema.enum.join(', ')}]` : ''}
    </span>
  );
}

// --- TryItPanel: Test endpoints from the dashboard ---

const MAX_RESPONSE_DISPLAY = 100 * 1024; // 100KB

function TryItPanel({
  endpoint,
  namespace,
  spec,
}: {
  endpoint: Endpoint;
  namespace: string;
  spec: any;
}) {
  const pathParamNames = useMemo(() => {
    const braceMatches = endpoint.path.match(/\{(\w+)\}/g);
    if (braceMatches) return braceMatches.map((m) => m.slice(1, -1));
    const colonMatches = endpoint.path.match(/(?<=\/):(\w+)/g);
    if (colonMatches) return colonMatches.map((m) => m.slice(1));
    return [];
  }, [endpoint.path]);

  const queryParams = useMemo(
    () => endpoint.parameters.filter((p) => p.in === 'query'),
    [endpoint.parameters],
  );

  const hasBody = ['post', 'put', 'patch'].includes(endpoint.method);
  const requestBodySchema = useMemo(
    () => (hasBody ? extractRequestBody(endpoint.operationRef, spec) : null),
    [hasBody, endpoint.operationRef, spec],
  );
  const defaultBody = useMemo(
    () => (requestBodySchema ? JSON.stringify(generateExample(requestBodySchema), null, 2) : ''),
    [requestBodySchema],
  );

  const [pathParams, setPathParams] = useState<Record<string, string>>({});
  const [queryValues, setQueryValues] = useState<Record<string, string>>({});
  const [apiKey, setApiKey] = useState('');
  const [bodyText, setBodyText] = useState(defaultBody);
  const [loading, setLoading] = useState(false);
  const [response, setResponse] = useState<{
    status: number;
    statusText: string;
    timeMs: number;
    body: string;
  } | null>(null);
  const [showFullBody, setShowFullBody] = useState(false);

  const buildUrl = () => {
    let path = endpoint.path;
    for (const name of pathParamNames) {
      const val = pathParams[name];
      const encoded = val ? encodeURIComponent(val) : `{${name}}`;
      path = path.replace(`{${name}}`, encoded).replace(`:${name}`, encoded);
    }
    const qs = Object.entries(queryValues)
      .filter(([, v]) => v)
      .map(([k, v]) => `${encodeURIComponent(k)}=${encodeURIComponent(v)}`)
      .join('&');
    return `/gateway/${namespace}${path}${qs ? '?' + qs : ''}`;
  };

  const handleSend = async () => {
    setLoading(true);
    setResponse(null);
    const url = buildUrl();
    const headers: Record<string, string> = {};
    if (apiKey) headers['X-Api-Key'] = apiKey;
    if (hasBody) headers['Content-Type'] = 'application/json';

    const start = performance.now();
    try {
      const res = await fetch(url, {
        method: endpoint.method.toUpperCase(),
        headers,
        body: hasBody && bodyText ? bodyText : undefined,
      });
      const elapsed = performance.now() - start;
      const text = await res.text();
      setResponse({ status: res.status, statusText: res.statusText, timeMs: elapsed, body: text });
    } catch (err: any) {
      const elapsed = performance.now() - start;
      setResponse({ status: 0, statusText: 'Network Error', timeMs: elapsed, body: err.message });
    } finally {
      setLoading(false);
    }
  };

  const formatBody = (body: string) => {
    try {
      return JSON.stringify(JSON.parse(body), null, 2);
    } catch {
      return body;
    }
  };

  return (
    <div className="space-y-3 mt-3 p-3 rounded-md bg-muted/30 border border-border">
      {/* URL */}
      <div className="flex items-center gap-2">
        <MethodBadge method={endpoint.method} />
        <code className="text-xs bg-background px-2 py-1 rounded border border-border flex-1 overflow-x-auto">
          {buildUrl()}
        </code>
      </div>

      {/* Path params */}
      {pathParamNames.length > 0 && (
        <div>
          <label className="text-xs font-semibold text-muted-foreground">Path Parameters</label>
          <div className="grid grid-cols-2 gap-2 mt-1">
            {pathParamNames.map((name) => (
              <div key={name} className="flex items-center gap-2">
                <label className="text-xs font-mono w-24 shrink-0">{name}</label>
                <input
                  className="flex-1 text-xs px-2 py-1 rounded border border-border bg-background"
                  placeholder={name}
                  value={pathParams[name] || ''}
                  onChange={(e) => setPathParams((prev) => ({ ...prev, [name]: e.target.value }))}
                />
              </div>
            ))}
          </div>
        </div>
      )}

      {/* Query params */}
      {queryParams.length > 0 && (
        <div>
          <label className="text-xs font-semibold text-muted-foreground">Query Parameters</label>
          <div className="grid grid-cols-2 gap-2 mt-1">
            {queryParams.map((p) => (
              <div key={p.name} className="flex items-center gap-2">
                <label className="text-xs font-mono w-24 shrink-0">
                  {p.name}
                  {p.required && <span className="text-red-500">*</span>}
                </label>
                <input
                  className="flex-1 text-xs px-2 py-1 rounded border border-border bg-background"
                  placeholder={p.description || p.name}
                  value={queryValues[p.name] || ''}
                  onChange={(e) =>
                    setQueryValues((prev) => ({ ...prev, [p.name]: e.target.value }))
                  }
                />
              </div>
            ))}
          </div>
        </div>
      )}

      {/* API Key */}
      <div>
        <label className="text-xs font-semibold text-muted-foreground">API Key (optional)</label>
        <input
          className="w-full text-xs px-2 py-1 mt-1 rounded border border-border bg-background"
          placeholder="X-Api-Key header value"
          value={apiKey}
          onChange={(e) => setApiKey(e.target.value)}
        />
      </div>

      {/* Request body */}
      {hasBody && (
        <div>
          <label className="text-xs font-semibold text-muted-foreground">Request Body</label>
          <textarea
            className="w-full text-xs font-mono px-2 py-1 mt-1 rounded border border-border bg-background min-h-[100px] resize-y"
            value={bodyText}
            onChange={(e) => setBodyText(e.target.value)}
          />
        </div>
      )}

      {/* Send */}
      <button
        onClick={handleSend}
        disabled={loading}
        className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-md text-xs font-medium bg-primary text-primary-foreground hover:bg-primary/90 disabled:opacity-50 cursor-pointer"
      >
        {loading ? <Loader2 className="w-3 h-3 animate-spin" /> : <Play className="w-3 h-3" />}
        Send
      </button>

      {/* Response */}
      {response && (
        <div className="space-y-1">
          <div className="flex items-center gap-3 text-xs">
            <span
              className={`font-mono font-bold ${
                response.status >= 200 && response.status < 300
                  ? 'text-green-600 dark:text-green-400'
                  : response.status >= 400 && response.status < 500
                    ? 'text-amber-600 dark:text-amber-400'
                    : response.status >= 500
                      ? 'text-red-600 dark:text-red-400'
                      : 'text-muted-foreground'
              }`}
            >
              {response.status} {response.statusText}
            </span>
            <span className="text-muted-foreground">{Math.round(response.timeMs)}ms</span>
          </div>
          <pre className="text-xs font-mono bg-background p-2 rounded border border-border overflow-x-auto max-h-[400px] overflow-y-auto whitespace-pre-wrap">
            {response.body.length > MAX_RESPONSE_DISPLAY && !showFullBody
              ? formatBody(response.body.slice(0, MAX_RESPONSE_DISPLAY)) + '\n...'
              : formatBody(response.body)}
          </pre>
          {response.body.length > MAX_RESPONSE_DISPLAY && (
            <button
              onClick={() => setShowFullBody(!showFullBody)}
              className="text-xs text-primary hover:underline cursor-pointer"
            >
              {showFullBody ? 'Truncate' : 'Show full response'}
            </button>
          )}
        </div>
      )}
    </div>
  );
}

// --- EndpointRow with schema display + Try It ---

function EndpointRow({
  endpoint,
  spec,
  namespace,
}: {
  endpoint: Endpoint;
  spec: any;
  namespace: string;
}) {
  const [expanded, setExpanded] = useState(false);
  const [showTryIt, setShowTryIt] = useState(false);
  const [expandedResponses, setExpandedResponses] = useState<Set<string>>(new Set());
  const hasDetails =
    endpoint.description || endpoint.parameters.length > 0 || endpoint.responses.length > 0 || endpoint.operationRef;

  const requestBodySchema = useMemo(
    () => (expanded ? extractRequestBody(endpoint.operationRef, spec) : null),
    [expanded, endpoint.operationRef, spec],
  );

  const responseSchemas = useMemo(
    () => (expanded ? extractResponseSchemas(endpoint.operationRef, spec) : {}),
    [expanded, endpoint.operationRef, spec],
  );

  const toggleResponse = (status: string) => {
    setExpandedResponses((prev) => {
      const next = new Set(prev);
      if (next.has(status)) next.delete(status);
      else next.add(status);
      return next;
    });
  };

  return (
    <div className="border-b border-border last:border-0">
      <button
        className="w-full flex items-center gap-3 px-4 py-3 hover:bg-muted/50 text-left cursor-pointer"
        onClick={() => hasDetails && setExpanded(!expanded)}
      >
        {hasDetails ? (
          expanded ? (
            <ChevronDown className="w-4 h-4 text-muted-foreground shrink-0" />
          ) : (
            <ChevronRight className="w-4 h-4 text-muted-foreground shrink-0" />
          )
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

          {/* Parameters */}
          {endpoint.parameters.length > 0 && (
            <div>
              <h4 className="text-xs font-semibold uppercase text-muted-foreground mb-1">
                Parameters
              </h4>
              <div className="overflow-x-auto">
                <table className="w-full text-xs">
                  <thead>
                    <tr className="border-b border-border">
                      <th className="text-left py-1 pr-4 font-medium text-muted-foreground">
                        Name
                      </th>
                      <th className="text-left py-1 pr-4 font-medium text-muted-foreground">In</th>
                      <th className="text-left py-1 pr-4 font-medium text-muted-foreground">
                        Type
                      </th>
                      <th className="text-left py-1 pr-4 font-medium text-muted-foreground">
                        Required
                      </th>
                      <th className="text-left py-1 font-medium text-muted-foreground">
                        Description
                      </th>
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

          {/* Request Body */}
          {requestBodySchema && (
            <div>
              <h4 className="text-xs font-semibold uppercase text-muted-foreground mb-1">
                Request Body
              </h4>
              <div className="bg-muted/30 rounded-md p-2 border border-border">
                <SchemaView schema={requestBodySchema} />
              </div>
            </div>
          )}

          {/* Responses */}
          {endpoint.responses.length > 0 && (
            <div>
              <h4 className="text-xs font-semibold uppercase text-muted-foreground mb-1">
                Responses
              </h4>
              <div className="space-y-1">
                {endpoint.responses.map((r, i) => {
                  const schema = responseSchemas[r.status];
                  const isExpanded = expandedResponses.has(r.status);
                  return (
                    <div key={i}>
                      <div
                        className={`flex items-center gap-2 text-xs ${schema ? 'cursor-pointer hover:bg-muted/50 rounded px-1 -mx-1' : ''}`}
                        onClick={() => schema && toggleResponse(r.status)}
                      >
                        {schema && (
                          isExpanded ? (
                            <ChevronDown className="w-3 h-3 text-muted-foreground shrink-0" />
                          ) : (
                            <ChevronRight className="w-3 h-3 text-muted-foreground shrink-0" />
                          )
                        )}
                        <span
                          className={`font-mono font-bold ${
                            r.status.startsWith('2')
                              ? 'text-green-600 dark:text-green-400'
                              : r.status.startsWith('4')
                                ? 'text-amber-600 dark:text-amber-400'
                                : r.status.startsWith('5')
                                  ? 'text-red-600 dark:text-red-400'
                                  : 'text-muted-foreground'
                          }`}
                        >
                          {r.status}
                        </span>
                        <span className="text-muted-foreground">{r.description}</span>
                      </div>
                      {schema && isExpanded && (
                        <div className="bg-muted/30 rounded-md p-2 border border-border mt-1 ml-5">
                          <SchemaView schema={schema} />
                        </div>
                      )}
                    </div>
                  );
                })}
              </div>
            </div>
          )}

          {/* Try It toggle */}
          <button
            onClick={() => setShowTryIt(!showTryIt)}
            className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-md text-xs font-medium border border-border hover:bg-muted cursor-pointer"
          >
            <Play className="w-3 h-3" />
            {showTryIt ? 'Hide' : 'Try It'}
          </button>

          {showTryIt && (
            <TryItPanel endpoint={endpoint} namespace={namespace} spec={spec} />
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
        <Link
          to="/services"
          className="text-sm text-primary hover:underline mt-2 inline-block"
        >
          Back to Services
        </Link>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center gap-4">
        <Link to="/services" className="p-2 hover:bg-muted rounded-md transition-colors">
          <ArrowLeft className="w-5 h-5" />
        </Link>
        <div className="flex-1">
          <div className="flex items-center gap-3">
            <h2 className="text-2xl font-bold">/{service.namespace}</h2>
            <Badge>v{service.version}</Badge>
            <span
              className={`inline-block px-2 py-0.5 rounded-full text-xs font-medium ${STATUS_COLORS[service.status] || ''}`}
            >
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
                <span className="text-xs text-muted-foreground">
                  Spec version: {spec.info.version}
                </span>
              )}
              {spec.info.description && (
                <p className="text-sm text-muted-foreground mt-1 max-w-2xl">
                  {spec.info.description}
                </p>
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
          {Object.entries(methodCounts)
            .sort()
            .map(([method, count]) => (
              <span
                key={method}
                className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-md text-sm"
              >
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
            <p className="text-xs text-muted-foreground">
              Re-import the service to view its API endpoints.
            </p>
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
                      <EndpointRow
                        key={`${ep.method}-${ep.path}-${i}`}
                        endpoint={ep}
                        spec={spec}
                        namespace={service.namespace}
                      />
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
