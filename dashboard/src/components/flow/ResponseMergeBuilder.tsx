import { useState, useEffect, useCallback } from 'react';
import { Input } from '../ui/input';
import { Label } from '../ui/label';
import { Button } from '../ui/button';
import { Badge } from '../ui/badge';
import { Plus, X, Wand2, Code, Eye, LayoutList } from 'lucide-react';
import type { StepForm, ServiceEndpoint } from './flowTypes';

interface MergeEntry {
  key: string;
  value: string;
}

interface ResponseMergeBuilderProps {
  value: string; // JSON string
  onChange: (json: string) => void;
  steps: StepForm[];
  serviceEndpoints?: Map<string, ServiceEndpoint[]>;
}

function parseToEntries(json: string): MergeEntry[] {
  try {
    const obj = JSON.parse(json);
    if (typeof obj !== 'object' || obj === null || Array.isArray(obj)) return [];
    return Object.entries(obj).map(([key, val]) => ({
      key,
      value: typeof val === 'string' ? val : JSON.stringify(val),
    }));
  } catch {
    return [];
  }
}

function entriesToJson(entries: MergeEntry[]): string {
  const obj: Record<string, any> = {};
  for (const e of entries) {
    if (!e.key.trim()) continue;
    const v = e.value;
    // Keep template expressions and plain strings as strings
    if (v.includes('${')) {
      obj[e.key] = v;
    } else {
      // Try parsing as JSON literal (number, boolean, null, object, array)
      try {
        obj[e.key] = JSON.parse(v);
      } catch {
        obj[e.key] = v;
      }
    }
  }
  return JSON.stringify(obj, null, 2);
}

const VALUE_SUFFIXES = ['.body', '.status', '.headers'];

export function ResponseMergeBuilder({ value, onChange, steps, serviceEndpoints }: ResponseMergeBuilderProps) {
  const [mode, setMode] = useState<'visual' | 'json'>('visual');
  const [entries, setEntries] = useState<MergeEntry[]>(() => parseToEntries(value));
  const [jsonText, setJsonText] = useState(value);
  const [previewOpen, setPreviewOpen] = useState(false);

  // Sync entries → JSON when entries change in visual mode
  const syncFromEntries = useCallback(
    (next: MergeEntry[]) => {
      setEntries(next);
      const json = entriesToJson(next);
      setJsonText(json);
      onChange(json);
    },
    [onChange]
  );

  // When switching to visual mode, parse current JSON
  useEffect(() => {
    if (mode === 'visual') {
      setEntries(parseToEntries(jsonText));
    }
  }, [mode]); // eslint-disable-line react-hooks/exhaustive-deps

  const updateEntry = (index: number, field: 'key' | 'value', val: string) => {
    const next = entries.map((e, i) => (i === index ? { ...e, [field]: val } : e));
    syncFromEntries(next);
  };

  const removeEntry = (index: number) => {
    syncFromEntries(entries.filter((_, i) => i !== index));
  };

  const addEntry = () => {
    syncFromEntries([...entries, { key: '', value: '' }]);
  };

  const autoGenerate = () => {
    const stepNames = steps.map((s) => s.name).filter(Boolean);
    if (stepNames.length === 0) return;
    const generated = stepNames.map((name) => ({
      key: name,
      value: `\${${name}.body}`,
    }));
    syncFromEntries(generated);
  };

  const handleJsonChange = (text: string) => {
    setJsonText(text);
    onChange(text);
  };

  const stepNames = steps.map((s) => s.name).filter(Boolean);

  // Build a map of step name → response schema from service endpoints
  const stepResponseSchemas = (() => {
    const map = new Map<string, Record<string, string>>();
    if (!serviceEndpoints) return map;
    for (const step of steps) {
      const eps = serviceEndpoints.get(step.upstream_id) ?? [];
      const ep = eps.find(e => e.method === step.method && (e.path === step.path_template || step.path_template.endsWith(e.path)));
      if (ep?.responseSchema) map.set(step.name, ep.responseSchema);
    }
    return map;
  })();

  // Build a sample preview of what the response would look like
  const buildPreview = () => {
    try {
      const obj = JSON.parse(mode === 'json' ? jsonText : entriesToJson(entries));
      const preview: Record<string, any> = {};
      for (const [key, val] of Object.entries(obj)) {
        if (typeof val === 'string' && val.includes('${')) {
          // Replace template expressions with sample data
          const replaced = val.replace(/\$\{([^}]+)\}/g, (_, expr: string) => {
            const parts = expr.split('.');
            if (parts[1] === 'status') return '200';
            if (parts[1] === 'headers') return '{ /* headers */ }';
            if (parts[1] === 'body' && parts.length > 2) {
              // e.g. ${post_add.body.AddResult} — resolve type from schema
              const fieldName = parts[2];
              const schema = stepResponseSchemas.get(parts[0]);
              const fieldType = schema?.[fieldName];
              if (fieldType === 'integer' || fieldType === 'number') return '0';
              if (fieldType === 'boolean') return 'true';
              if (fieldType === 'array') return '[]';
              if (fieldType === 'string') return `"<${fieldName}>"`;
              return `<${fieldName}>`;
            }
            if (parts[1] === 'body') {
              // ${step.body} — show all known response fields or generic
              const schema = stepResponseSchemas.get(parts[0]);
              if (schema && Object.keys(schema).length > 0) {
                const sample: Record<string, any> = {};
                for (const [f, t] of Object.entries(schema)) {
                  if (t === 'integer' || t === 'number') sample[f] = 0;
                  else if (t === 'boolean') sample[f] = true;
                  else if (t === 'array') sample[f] = [];
                  else if (t === 'string') sample[f] = '';
                  else sample[f] = {};
                }
                return JSON.stringify(sample);
              }
              return `{ /* ${parts[0]} response */ }`;
            }
            return `<${expr}>`;
          });
          // Try to parse the replaced string as JSON for cleaner output
          try { preview[key] = JSON.parse(replaced); } catch { preview[key] = replaced; }
        } else {
          preview[key] = val;
        }
      }
      return JSON.stringify(preview, null, 2);
    } catch {
      return 'Invalid JSON';
    }
  };

  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between">
        <Label>Response Merge Template</Label>
        <div className="flex gap-1">
          <button
            type="button"
            onClick={() => setMode('visual')}
            className={`px-2 py-1 text-xs rounded-l border cursor-pointer ${
              mode === 'visual'
                ? 'bg-primary text-primary-foreground border-primary'
                : 'border-border hover:bg-muted'
            }`}
          >
            <LayoutList className="w-3 h-3 inline mr-1" />
            Visual
          </button>
          <button
            type="button"
            onClick={() => setMode('json')}
            className={`px-2 py-1 text-xs rounded-r border-t border-r border-b cursor-pointer ${
              mode === 'json'
                ? 'bg-primary text-primary-foreground border-primary'
                : 'border-border hover:bg-muted'
            }`}
          >
            <Code className="w-3 h-3 inline mr-1" />
            JSON
          </button>
        </div>
      </div>

      {mode === 'visual' ? (
        <div className="space-y-2">
          <div className="flex gap-2">
            <Button type="button" variant="secondary" size="sm" onClick={autoGenerate}>
              <Wand2 className="w-3 h-3 mr-1" /> Auto-generate from steps
            </Button>
            <Button type="button" variant="secondary" size="sm" onClick={() => setPreviewOpen(!previewOpen)}>
              <Eye className="w-3 h-3 mr-1" /> {previewOpen ? 'Hide' : 'Preview'}
            </Button>
          </div>

          {entries.length === 0 ? (
            <div className="border border-dashed border-border rounded-md p-4 text-center text-sm text-muted-foreground">
              No merge entries. Click "Auto-generate" to create from step names, or add manually.
            </div>
          ) : (
            <div className="space-y-1.5">
              <div className="grid grid-cols-[1fr_2fr_auto] gap-2 text-xs text-muted-foreground px-1">
                <span>Key (response field)</span>
                <span>Value (step ref or static)</span>
                <span className="w-6"></span>
              </div>
              {entries.map((entry, i) => (
                <div key={i} className="grid grid-cols-[1fr_2fr_auto] gap-2 items-center">
                  <Input
                    value={entry.key}
                    onChange={(e) => updateEntry(i, 'key', e.target.value)}
                    placeholder="user"
                    className="h-8 text-xs font-mono"
                  />
                  <div className="relative">
                    <Input
                      value={entry.value}
                      onChange={(e) => updateEntry(i, 'value', e.target.value)}
                      placeholder="${step.body} or static value"
                      className="h-8 text-xs font-mono"
                    />
                    {stepNames.length > 0 && !entry.value && (
                      <div className="absolute right-1 top-1 flex gap-0.5">
                        {stepNames.slice(0, 3).map((name) => (
                          <button
                            key={name}
                            type="button"
                            onClick={() => updateEntry(i, 'value', `\${${name}.body}`)}
                            className="px-1.5 py-0.5 text-[10px] rounded bg-muted hover:bg-accent border border-border cursor-pointer"
                          >
                            {name}
                          </button>
                        ))}
                      </div>
                    )}
                  </div>
                  <button
                    type="button"
                    onClick={() => removeEntry(i)}
                    className="p-1 hover:bg-muted rounded text-destructive cursor-pointer"
                  >
                    <X className="w-3.5 h-3.5" />
                  </button>
                </div>
              ))}
            </div>
          )}

          <Button type="button" variant="ghost" size="sm" onClick={addEntry}>
            <Plus className="w-3 h-3 mr-1" /> Add field
          </Button>

          {/* Available step references */}
          {stepNames.length > 0 && (
            <div className="flex items-center gap-1.5 text-xs text-muted-foreground">
              <span>Available steps:</span>
              {stepNames.map((name) => (
                <Badge key={name} variant="muted" className="text-[10px] font-mono">
                  {name}
                </Badge>
              ))}
              <span className="text-[10px]">(.body, .status, .headers) — or use static values like "ok", 42, true</span>
            </div>
          )}

          {previewOpen && (
            <div className="border border-border rounded-md bg-muted/30 p-3">
              <p className="text-xs font-medium text-muted-foreground mb-1">Response Preview</p>
              <pre className="text-xs font-mono text-foreground whitespace-pre-wrap">{buildPreview()}</pre>
            </div>
          )}
        </div>
      ) : (
        <div className="space-y-2">
          <textarea
            value={jsonText}
            onChange={(e) => handleJsonChange(e.target.value)}
            placeholder='{"user": "${user.body}", "orders": "${orders.body}"}'
            className="w-full h-24 px-3 py-2 text-xs font-mono border border-border rounded-md bg-background resize-y"
          />
          <Button type="button" variant="secondary" size="sm" onClick={() => setPreviewOpen(!previewOpen)}>
            <Eye className="w-3 h-3 mr-1" /> {previewOpen ? 'Hide' : 'Preview'}
          </Button>
          {previewOpen && (
            <div className="border border-border rounded-md bg-muted/30 p-3">
              <p className="text-xs font-medium text-muted-foreground mb-1">Response Preview</p>
              <pre className="text-xs font-mono text-foreground whitespace-pre-wrap">{buildPreview()}</pre>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
