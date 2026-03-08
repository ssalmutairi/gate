import { useState, useCallback } from 'react';
import { Input } from '../ui/input';
import { Button } from '../ui/button';
import { Plus, X, Code, LayoutList } from 'lucide-react';

interface BodyTemplateBuilderProps {
  value: string; // JSON string
  onChange: (json: string) => void;
}

interface FieldEntry {
  key: string;
  value: string;
}

function parseToFields(json: string): FieldEntry[] {
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

function fieldsToJson(fields: FieldEntry[]): string {
  const obj: Record<string, string> = {};
  for (const f of fields) {
    if (f.key.trim()) obj[f.key] = f.value;
  }
  return JSON.stringify(obj, null, 2);
}

export function BodyTemplateBuilder({ value, onChange }: BodyTemplateBuilderProps) {
  const [mode, setMode] = useState<'visual' | 'json'>(() => {
    // Start in visual if value is valid JSON object, else json
    try {
      const obj = JSON.parse(value || '{}');
      return typeof obj === 'object' && obj !== null && !Array.isArray(obj) ? 'visual' : 'json';
    } catch {
      return value.trim() ? 'json' : 'visual';
    }
  });
  const [fields, setFields] = useState<FieldEntry[]>(() => parseToFields(value || '{}'));

  const syncFromFields = useCallback(
    (next: FieldEntry[]) => {
      setFields(next);
      onChange(fieldsToJson(next));
    },
    [onChange]
  );

  const updateField = (index: number, part: 'key' | 'value', val: string) => {
    const next = fields.map((f, i) => (i === index ? { ...f, [part]: val } : f));
    syncFromFields(next);
  };

  const removeField = (index: number) => {
    syncFromFields(fields.filter((_, i) => i !== index));
  };

  const addField = () => {
    syncFromFields([...fields, { key: '', value: '' }]);
  };

  const switchToJson = () => {
    setMode('json');
  };

  const switchToVisual = () => {
    setFields(parseToFields(value || '{}'));
    setMode('visual');
  };

  return (
    <div className="space-y-1">
      <div className="flex items-center justify-between">
        <span className="text-xs font-medium">Body Template</span>
        <div className="flex">
          <button
            type="button"
            onClick={switchToVisual}
            className={`px-1.5 py-0.5 text-[10px] rounded-l border cursor-pointer ${
              mode === 'visual'
                ? 'bg-primary text-primary-foreground border-primary'
                : 'border-border hover:bg-muted'
            }`}
          >
            <LayoutList className="w-2.5 h-2.5 inline" />
          </button>
          <button
            type="button"
            onClick={switchToJson}
            className={`px-1.5 py-0.5 text-[10px] rounded-r border-t border-r border-b cursor-pointer ${
              mode === 'json'
                ? 'bg-primary text-primary-foreground border-primary'
                : 'border-border hover:bg-muted'
            }`}
          >
            <Code className="w-2.5 h-2.5 inline" />
          </button>
        </div>
      </div>

      {mode === 'visual' ? (
        <div className="space-y-1">
          {fields.map((f, i) => (
            <div key={i} className="flex items-center gap-1">
              <Input
                value={f.key}
                onChange={(e) => updateField(i, 'key', e.target.value)}
                placeholder="field"
                className="h-6 text-[11px] font-mono flex-1"
              />
              <Input
                value={f.value}
                onChange={(e) => updateField(i, 'value', e.target.value)}
                placeholder="${request.body.field}"
                className="h-6 text-[11px] font-mono flex-[2]"
              />
              <button
                type="button"
                onClick={() => removeField(i)}
                className="p-0.5 hover:bg-muted rounded text-destructive cursor-pointer shrink-0"
              >
                <X className="w-3 h-3" />
              </button>
            </div>
          ))}
          <Button type="button" variant="ghost" size="sm" onClick={addField} className="h-5 text-[10px] px-1.5">
            <Plus className="w-2.5 h-2.5 mr-0.5" /> Add field
          </Button>
        </div>
      ) : (
        <textarea
          value={value}
          onChange={(e) => onChange(e.target.value)}
          placeholder='{"email": "${user.body.email}"}'
          className="w-full h-14 px-2 py-1 text-xs font-mono border border-border rounded-md bg-background resize-y"
        />
      )}
    </div>
  );
}
