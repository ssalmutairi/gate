import { useState, useRef, useEffect } from 'react';
import { Button } from '../ui/button';
import { Input } from '../ui/input';
import { Label } from '../ui/label';
import { Badge } from '../ui/badge';
import { Card } from '../ui/card';
import { Play, ChevronDown, ChevronUp, Copy, Check, Key } from 'lucide-react';
import { toast } from 'sonner';
import axios from 'axios';

interface TestEndpointPanelProps {
  pathPrefix: string;
  pathPattern: string;
  methods: string[];
  /** When true, the panel starts expanded (no collapse header). */
  defaultOpen?: boolean;
  /** When true, uses a stacked vertical layout for narrow panels. */
  compact?: boolean;
  /** Called before the first test if the composition hasn't been saved yet. Should save and return true, or false on failure. */
  onSaveBeforeTest?: () => Promise<boolean>;
  /** Controlled request body — lifted to parent so state persists across wizard steps. */
  requestBody: string;
  onRequestBodyChange: (body: string) => void;
  /** Last test result — lifted to parent so it persists across wizard steps. */
  lastResult?: TestResult | null;
  onResultChange?: (result: TestResult | null) => void;
  /** Optional input schema for typed form fields. */
  inputSchema?: any;
}

export interface TestResult {
  status: number;
  statusText: string;
  latencyMs: number;
  headers: Record<string, string>;
  body: any;
  size: string;
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function TestEndpointPanel({ pathPrefix, pathPattern, methods, defaultOpen, compact, onSaveBeforeTest, requestBody, onRequestBodyChange, lastResult, onResultChange, inputSchema }: TestEndpointPanelProps) {
  const [open, setOpen] = useState(defaultOpen ?? false);
  const [method, setMethod] = useState(methods[0] || 'GET');
  const [pathParams, setPathParams] = useState('');
  const [apiKey, setApiKey] = useState('');
  const [loading, setLoading] = useState(false);
  const [result, setResult] = useState<TestResult | null>(lastResult ?? null);
  const [copied, setCopied] = useState(false);
  const abortRef = useRef<AbortController | null>(null);
  const [useTypedForm, setUseTypedForm] = useState(true);
  const [formValues, setFormValues] = useState<Record<string, any>>({});

  const setRequestBody = onRequestBodyChange;
  const schemaProperties = inputSchema?.properties as Record<string, any> | undefined;
  const hasSchemaForm = useTypedForm && schemaProperties && Object.keys(schemaProperties).length > 0;

  const updateFormField = (field: string, value: any) => {
    const next = { ...formValues, [field]: value };
    setFormValues(next);
    onRequestBodyChange(JSON.stringify(next, null, 2));
  };
  const setResultAndNotify = (r: TestResult | null) => {
    setResult(r);
    onResultChange?.(r);
  };

  useEffect(() => {
    if (methods.length > 0) setMethod(methods[0]);
  }, [methods.join(',')]);

  const effectiveMethods = methods.length > 0 ? methods : ['GET', 'POST', 'PUT', 'PATCH', 'DELETE'];
  const fullPath = `/gateway${pathPrefix}${pathParams || pathPattern || ''}`;
  const hasBody = ['POST', 'PUT', 'PATCH'].includes(method);

  const handleTest = async () => {
    // Auto-save composition before every test to ensure latest changes are applied
    if (onSaveBeforeTest) {
      setLoading(true);
      const ok = await onSaveBeforeTest();
      if (!ok) {
        setLoading(false);
        return;
      }
      // Trigger immediate config reload on the proxy
      try { await axios.post('/admin/reload'); } catch { /* ignore */ }
    }

    if (abortRef.current) abortRef.current.abort();
    const controller = new AbortController();
    abortRef.current = controller;

    setLoading(true);
    setResultAndNotify(null);

    const start = performance.now();
    try {
      const config: any = {
        method: method.toLowerCase(),
        url: fullPath,
        signal: controller.signal,
        validateStatus: () => true, // don't throw on 4xx/5xx
        headers: {
          'Content-Type': 'application/json',
          ...(apiKey ? { 'X-Api-Key': apiKey } : {}),
        },
      };

      if (hasBody && requestBody.trim()) {
        try {
          config.data = JSON.parse(requestBody);
        } catch {
          toast.error('Invalid JSON in request body');
          setLoading(false);
          return;
        }
      }

      const res = await axios(config);
      const latencyMs = Math.round(performance.now() - start);
      const bodyStr = typeof res.data === 'string' ? res.data : JSON.stringify(res.data, null, 2);
      const size = new Blob([bodyStr]).size;

      setResultAndNotify({
        status: res.status,
        statusText: res.statusText,
        latencyMs,
        headers: Object.fromEntries(
          Object.entries(res.headers).filter(([, v]) => typeof v === 'string') as [string, string][]
        ),
        body: res.data,
        size: formatBytes(size),
      });
    } catch (err: any) {
      if (err.name === 'CanceledError') return;
      setResultAndNotify({
        status: 0,
        statusText: 'Network Error',
        latencyMs: Math.round(performance.now() - start),
        headers: {},
        body: { error: err.message },
        size: '0 B',
      });
    } finally {
      setLoading(false);
    }
  };

  const copyResponse = () => {
    if (!result) return;
    const text = typeof result.body === 'string' ? result.body : JSON.stringify(result.body, null, 2);
    navigator.clipboard.writeText(text);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  const statusColor = (status: number) => {
    if (status >= 200 && status < 300) return 'text-green-500';
    if (status >= 300 && status < 400) return 'text-yellow-500';
    if (status >= 400) return 'text-red-500';
    return 'text-muted-foreground';
  };

  return (
    <Card className="overflow-hidden">
      <button
        type="button"
        onClick={() => setOpen(!open)}
        className="w-full flex items-center justify-between px-4 py-3 hover:bg-muted/50 cursor-pointer"
      >
        <div className="flex items-center gap-2">
          <Play className="w-4 h-4 text-primary" />
          <span className="font-medium text-sm">Test Endpoint</span>
          {result && (
            <Badge variant={result.status >= 200 && result.status < 300 ? 'default' : 'destructive'} className="text-xs">
              {result.status} - {result.latencyMs}ms
            </Badge>
          )}
        </div>
        {open ? <ChevronUp className="w-4 h-4" /> : <ChevronDown className="w-4 h-4" />}
      </button>

      {open && (
        <div
          className="px-4 pb-4 border-t border-border space-y-3"
          onKeyDown={(e) => {
            if (e.key === 'Enter') {
              e.preventDefault();
              e.stopPropagation();
              if (!loading && pathPrefix) handleTest();
            }
          }}
        >
          {/* Request builder */}
          {compact ? (
            <div className="space-y-2 mt-3">
              <div className="flex items-center gap-2">
                <select
                  value={method}
                  onChange={(e) => setMethod(e.target.value)}
                  className="h-8 text-xs px-2 border border-border rounded-md bg-background shrink-0"
                >
                  {effectiveMethods.map((m) => (
                    <option key={m} value={m}>{m}</option>
                  ))}
                </select>
                <div className="flex-1" />
                <Button
                  type="button"
                  size="sm"
                  onClick={handleTest}
                  disabled={loading}
                  className="h-8 shrink-0"
                >
                  <Play className="w-3 h-3 mr-1" />
                  {loading ? '...' : 'Send'}
                </Button>
              </div>
              <div className="flex items-center gap-0">
                <span className="h-8 flex items-center px-2 text-xs font-mono bg-muted border border-r-0 border-border rounded-l-md text-muted-foreground shrink-0 whitespace-nowrap">
                  {pathPrefix}
                </span>
                <Input
                  value={pathParams}
                  onChange={(e) => setPathParams(e.target.value)}
                  placeholder={pathPattern || '/'}
                  className="h-8 text-xs font-mono rounded-l-none"
                />
              </div>
            </div>
          ) : (
            <div className="flex items-end gap-2 mt-3">
              <div className="space-y-1">
                <Label className="text-xs">Method</Label>
                <select
                  value={method}
                  onChange={(e) => setMethod(e.target.value)}
                  className="h-8 text-xs px-2 border border-border rounded-md bg-background"
                >
                  {effectiveMethods.map((m) => (
                    <option key={m} value={m}>{m}</option>
                  ))}
                </select>
              </div>
              <div className="flex-1 space-y-1">
                <Label className="text-xs">URL</Label>
                <div className="flex items-center gap-0">
                  <span className="h-8 flex items-center px-2 text-xs font-mono bg-muted border border-r-0 border-border rounded-l-md text-muted-foreground whitespace-nowrap">
                    {pathPrefix}
                  </span>
                  <Input
                    value={pathParams}
                    onChange={(e) => setPathParams(e.target.value)}
                    placeholder={pathPattern || '/'}
                    className="h-8 text-xs font-mono rounded-l-none"
                  />
                </div>
              </div>
              <Button
                type="button"
                size="sm"
                onClick={handleTest}
                disabled={loading}
                className="h-8"
              >
                <Play className="w-3 h-3 mr-1" />
                {loading ? 'Sending...' : 'Send'}
              </Button>
            </div>
          )}

          {/* API Key */}
          <div className="space-y-1">
            <Label className="text-xs flex items-center gap-1">
              <Key className="w-3 h-3" /> API Key
              <span className="text-muted-foreground font-normal">(optional)</span>
            </Label>
            <Input
              type="password"
              value={apiKey}
              onChange={(e) => setApiKey(e.target.value)}
              placeholder="gw_..."
              className="h-8 text-xs font-mono"
            />
          </div>

          {/* Request body */}
          {hasBody && (
            <div className="space-y-1">
              <div className="flex items-center justify-between">
                <Label className="text-xs">Request Body</Label>
                {schemaProperties && Object.keys(schemaProperties).length > 0 && (
                  <button
                    type="button"
                    onClick={() => setUseTypedForm(!useTypedForm)}
                    className="text-[10px] text-primary hover:underline cursor-pointer"
                  >
                    {useTypedForm ? 'Raw JSON' : 'Typed Form'}
                  </button>
                )}
              </div>
              {hasSchemaForm ? (
                <div className="space-y-2 border border-border rounded-md p-2">
                  {Object.entries(schemaProperties!).map(([field, schema]) => {
                    const fieldType = (schema as any)?.type ?? 'string';
                    const value = formValues[field] ?? '';
                    return (
                      <div key={field} className="space-y-0.5">
                        <label className="text-[10px] text-muted-foreground font-mono">{field} <span className="text-[9px] opacity-50">({fieldType})</span></label>
                        {fieldType === 'boolean' ? (
                          <label className="flex items-center gap-1.5 text-xs cursor-pointer">
                            <input
                              type="checkbox"
                              checked={!!value}
                              onChange={(e) => updateFormField(field, e.target.checked)}
                              className="rounded border-border"
                            />
                            <span>{value ? 'true' : 'false'}</span>
                          </label>
                        ) : fieldType === 'number' || fieldType === 'integer' ? (
                          <Input
                            type="number"
                            value={value}
                            onChange={(e) => updateFormField(field, e.target.value === '' ? '' : Number(e.target.value))}
                            className="h-7 text-xs font-mono"
                            step={fieldType === 'integer' ? 1 : 'any'}
                          />
                        ) : (
                          <Input
                            type="text"
                            value={value}
                            onChange={(e) => updateFormField(field, e.target.value)}
                            className="h-7 text-xs font-mono"
                          />
                        )}
                      </div>
                    );
                  })}
                </div>
              ) : (
                <textarea
                  value={requestBody}
                  onChange={(e) => setRequestBody(e.target.value)}
                  placeholder='{"title": "foo", "body": "bar", "userId": 1}'
                  className="w-full h-24 px-3 py-2 text-xs font-mono border border-border rounded-md bg-background resize-y"
                />
              )}
            </div>
          )}

          {/* Response */}
          {result && (
            <div className="space-y-2">
              <div className="flex items-center gap-3 text-xs">
                <span className={`font-bold ${statusColor(result.status)}`}>
                  {result.status} {result.statusText}
                </span>
                <span className="text-muted-foreground">{result.latencyMs}ms</span>
                <span className="text-muted-foreground">{result.size}</span>
                <button
                  type="button"
                  onClick={copyResponse}
                  className="ml-auto p-1 hover:bg-muted rounded cursor-pointer"
                  title="Copy response"
                >
                  {copied ? <Check className="w-3 h-3 text-green-500" /> : <Copy className="w-3 h-3" />}
                </button>
              </div>

              {/* Response headers (collapsible) */}
              <details className="text-xs">
                <summary className="cursor-pointer text-muted-foreground hover:text-foreground">
                  Response Headers ({Object.keys(result.headers).length})
                </summary>
                <div className="mt-1 font-mono bg-muted/30 border border-border rounded-md p-2 max-h-32 overflow-auto">
                  {Object.entries(result.headers).map(([k, v]) => (
                    <div key={k}>
                      <span className="text-muted-foreground">{k}:</span> {v}
                    </div>
                  ))}
                </div>
              </details>

              {/* Response body */}
              <pre className="text-xs font-mono bg-muted/30 border border-border rounded-md p-3 max-h-80 overflow-auto whitespace-pre-wrap">
                {typeof result.body === 'string' ? result.body : JSON.stringify(result.body, null, 2)}
              </pre>
            </div>
          )}

          {onSaveBeforeTest && (
            <p className="text-xs text-muted-foreground">
              The composition will be auto-saved when you send a test request.
            </p>
          )}
        </div>
      )}
    </Card>
  );
}
