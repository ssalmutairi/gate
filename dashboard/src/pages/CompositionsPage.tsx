import { useState, useEffect, useMemo } from 'react';
import { useNavigate } from 'react-router-dom';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import {
  getCompositions,
  getUpstreams,
  updateComposition,
  deleteComposition,
  getComposition,
  type Composition,
  type CompositionStep,
  type Upstream,
} from '../lib/api';
import { Button } from '../components/ui/button';
import { Card } from '../components/ui/card';
import { Badge } from '../components/ui/badge';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from '../components/ui/dialog';
import { EmptyState } from '../components/ui';
import { Switch } from '../components/ui/switch';
import { TestEndpointPanel } from '../components/flow/TestEndpointPanel';
import { toast } from 'sonner';
import { Plus, Pencil, Trash2, ChevronDown, ChevronUp, Play, ChevronRight, FileJson } from 'lucide-react';

interface NamespaceGroup {
  namespace: string | null;
  compositions: Composition[];
}

export default function CompositionsPage() {
  const navigate = useNavigate();
  const qc = useQueryClient();
  const compositions = useQuery({ queryKey: ['compositions'], queryFn: getCompositions });
  const upstreams = useQuery({ queryKey: ['upstreams'], queryFn: getUpstreams });
  const [deleting, setDeleting] = useState<Composition | null>(null);
  const [testing, setTesting] = useState<Composition | null>(null);
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const [collapsedNamespaces, setCollapsedNamespaces] = useState<Set<string>>(new Set());

  const deleteMut = useMutation({
    mutationFn: deleteComposition,
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['compositions'] });
      setDeleting(null);
      toast.success('Composition deleted');
    },
    onError: (e: any) => toast.error(e.response?.data?.error ?? 'Failed to delete composition'),
  });

  const toggleActive = useMutation({
    mutationFn: ({ id, active }: { id: string; active: boolean }) =>
      updateComposition(id, { active }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['compositions'] });
      toast.success('Composition status updated');
    },
  });

  const upstreamName = (id: string) =>
    upstreams.data?.find((u: Upstream) => u.id === id)?.name ?? id.slice(0, 8);

  const namespaceGroups = useMemo<NamespaceGroup[]>(() => {
    if (!compositions.data) return [];
    const map = new Map<string | null, Composition[]>();
    for (const comp of compositions.data) {
      const ns = comp.namespace ?? null;
      if (!map.has(ns)) map.set(ns, []);
      map.get(ns)!.push(comp);
    }
    // Sort: named namespaces first (alphabetically), ungrouped last
    const entries = Array.from(map.entries());
    entries.sort(([a], [b]) => {
      if (a === null && b === null) return 0;
      if (a === null) return 1;
      if (b === null) return -1;
      return a.localeCompare(b);
    });
    return entries.map(([namespace, comps]) => ({ namespace, compositions: comps }));
  }, [compositions.data]);

  const toggleNamespace = (ns: string) => {
    setCollapsedNamespaces(prev => {
      const next = new Set(prev);
      if (next.has(ns)) next.delete(ns);
      else next.add(ns);
      return next;
    });
  };

  const nsKey = (ns: string | null) => ns ?? '_ungrouped';

  return (
    <div>
      <div className="flex items-center justify-between mb-6">
        <h1 className="text-2xl font-bold">Compositions</h1>
        <Button onClick={() => navigate('/compositions/new')}>
          <Plus className="w-4 h-4 mr-1" /> Create Composition
        </Button>
      </div>

      {compositions.data?.length === 0 ? (
        <Card>
          <EmptyState
            message="No compositions configured yet."
            action={<Button onClick={() => navigate('/compositions/new')}>Create your first composition</Button>}
          />
        </Card>
      ) : (
        <div className="space-y-4">
          {namespaceGroups.map((group) => {
            const key = nsKey(group.namespace);
            const isCollapsed = collapsedNamespaces.has(key);
            const displayName = group.namespace ?? 'Ungrouped';

            return (
              <Card key={key} className="overflow-hidden">
                {/* Namespace header */}
                <div
                  className="flex items-center justify-between px-4 py-3 bg-muted/30 border-b border-border cursor-pointer select-none"
                  onClick={() => toggleNamespace(key)}
                >
                  <div className="flex items-center gap-2">
                    {isCollapsed ? (
                      <ChevronRight className="w-4 h-4 text-muted-foreground" />
                    ) : (
                      <ChevronDown className="w-4 h-4 text-muted-foreground" />
                    )}
                    <span className="font-semibold text-sm">{displayName}</span>
                    <Badge variant="muted" className="text-[10px]">
                      {group.compositions.length} composition{group.compositions.length !== 1 ? 's' : ''}
                    </Badge>
                  </div>
                  <Button
                    variant="ghost"
                    size="sm"
                    className="text-xs gap-1"
                    onClick={(e) => {
                      e.stopPropagation();
                      navigate(`/compositions/namespaces/${encodeURIComponent(key)}`);
                    }}
                  >
                    <FileJson className="w-3.5 h-3.5" />
                    View OpenAPI
                  </Button>
                </div>

                {/* Compositions table */}
                {!isCollapsed && (
                  <div className="overflow-x-auto">
                    <table className="w-full text-sm">
                      <thead>
                        <tr className="border-b border-border text-left text-muted-foreground">
                          <th className="px-4 py-3 font-medium w-8"></th>
                          <th className="px-4 py-3 font-medium">Name</th>
                          <th className="px-4 py-3 font-medium">Path</th>
                          <th className="px-4 py-3 font-medium">Methods</th>
                          <th className="px-4 py-3 font-medium">Timeout</th>
                          <th className="px-4 py-3 font-medium">Auth</th>
                          <th className="px-4 py-3 font-medium">Enabled</th>
                          <th className="px-4 py-3 font-medium w-28"></th>
                        </tr>
                      </thead>
                      <tbody>
                        {group.compositions.map((comp) => (
                          <CompositionRow
                            key={comp.id}
                            comp={comp}
                            expanded={expandedId === comp.id}
                            onToggleExpand={() => setExpandedId(expandedId === comp.id ? null : comp.id)}
                            onEdit={() => navigate(`/compositions/${comp.id}/edit`)}
                            onTest={() => setTesting(comp)}
                            onDelete={() => setDeleting(comp)}
                            onToggleActive={(active) => toggleActive.mutate({ id: comp.id, active })}
                            upstreamName={upstreamName}
                          />
                        ))}
                      </tbody>
                    </table>
                  </div>
                )}
              </Card>
            );
          })}
        </div>
      )}

      {/* Test Dialog */}
      {testing && (
        <TestDialog comp={testing} onClose={() => setTesting(null)} />
      )}

      {/* Delete Confirmation */}
      <Dialog open={!!deleting} onOpenChange={(open) => !open && setDeleting(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Delete Composition</DialogTitle>
            <DialogDescription>
              Are you sure you want to delete "{deleting?.name}"? This action cannot be undone.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="secondary" onClick={() => setDeleting(null)}>Cancel</Button>
            <Button variant="destructive" onClick={() => deleting && deleteMut.mutate(deleting.id)}>Delete</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}

/** Extract ${request.body.X} fields from steps' body_template and path_template. */
function buildSampleBodyFromSteps(steps: CompositionStep[]): string {
  const fields = new Map<string, string>(); // field name -> guessed type

  for (const step of steps) {
    // Extract from body_template
    if (step.body_template) {
      let tpl: Record<string, any>;
      try {
        tpl = typeof step.body_template === 'string'
          ? JSON.parse(step.body_template)
          : step.body_template;
      } catch { continue; }

      for (const [, val] of Object.entries(tpl)) {
        if (typeof val !== 'string') continue;
        const match = val.match(/^\$\{request\.body\.(\w+)\}$/);
        if (match && !fields.has(match[1])) {
          fields.set(match[1], 'string');
        }
      }
    }

    // Extract from path_template: ${request.query.X} or ${request.path.X}
    const pathRefs = step.path_template?.matchAll(/\$\{request\.(query|path)\.(\w+)\}/g);
    if (pathRefs) {
      for (const m of pathRefs) {
        // path/query params aren't in body, just note them
      }
    }
  }

  if (fields.size === 0) return '';

  const obj: Record<string, any> = {};
  for (const [name] of fields) {
    obj[name] = '';
  }
  return JSON.stringify(obj, null, 2);
}

/** Format response_merge template as a readable expected output hint. */
function TestDialog({ comp, onClose }: { comp: Composition; onClose: () => void }) {
  const detail = useQuery({
    queryKey: ['composition', comp.id],
    queryFn: () => getComposition(comp.id),
  });

  const [testBody, setTestBody] = useState('');
  const [initialized, setInitialized] = useState(false);

  // Auto-generate sample body once steps are loaded
  useEffect(() => {
    if (initialized || !detail.data?.steps?.length) return;
    const sample = buildSampleBodyFromSteps(detail.data.steps);
    if (sample) setTestBody(sample);
    setInitialized(true);
  }, [detail.data, initialized]);

  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="max-w-2xl max-h-[85vh] overflow-y-auto">
        <DialogHeader>
          <DialogTitle>Test: {comp.name}</DialogTitle>
          <DialogDescription>
            Send a request to the saved composition. No changes will be saved.
          </DialogDescription>
        </DialogHeader>

        <TestEndpointPanel
          pathPrefix={comp.path_prefix}
          pathPattern={comp.path_pattern ?? ''}
          methods={comp.methods ?? []}
          defaultOpen
          requestBody={testBody}
          onRequestBodyChange={setTestBody}
          inputSchema={comp.input_schema}
        />
      </DialogContent>
    </Dialog>
  );
}

function CompositionRow({
  comp,
  expanded,
  onToggleExpand,
  onEdit,
  onTest,
  onDelete,
  onToggleActive,
  upstreamName,
}: {
  comp: Composition;
  expanded: boolean;
  onToggleExpand: () => void;
  onEdit: () => void;
  onTest: () => void;
  onDelete: () => void;
  onToggleActive: (active: boolean) => void;
  upstreamName: (id: string) => string;
}) {
  const detail = useQuery({
    queryKey: ['composition', comp.id],
    queryFn: () => getComposition(comp.id),
    enabled: expanded,
  });

  return (
    <>
      <tr className={`border-b border-border last:border-0 hover:bg-muted/50 ${!comp.active ? 'opacity-50' : ''}`}>
        <td className="px-4 py-3">
          <button onClick={onToggleExpand} className="p-0.5 hover:bg-muted rounded cursor-pointer">
            {expanded ? <ChevronUp className="w-4 h-4" /> : <ChevronDown className="w-4 h-4" />}
          </button>
        </td>
        <td className="px-4 py-3 font-medium">{comp.name}</td>
        <td className="px-4 py-3 font-mono text-xs">
          {comp.path_prefix}
          {comp.path_pattern && <span className="text-muted-foreground">{comp.path_pattern}</span>}
        </td>
        <td className="px-4 py-3">
          {comp.methods?.length ? (
            <div className="flex gap-1 flex-wrap">
              {comp.methods.map((m: string) => (
                <Badge key={m} variant="muted">{m}</Badge>
              ))}
            </div>
          ) : (
            <span className="text-muted-foreground">All</span>
          )}
        </td>
        <td className="px-4 py-3 text-xs">
          {comp.timeout_ms}ms
          {comp.max_wait_ms && <span className="text-muted-foreground ml-1">(eager: {comp.max_wait_ms}ms)</span>}
        </td>
        <td className="px-4 py-3">
          {comp.auth_skip ? (
            <Badge variant="muted">Skipped</Badge>
          ) : (
            <Badge variant="default">Enforced</Badge>
          )}
        </td>
        <td className="px-4 py-3">
          <Switch checked={comp.active} onCheckedChange={onToggleActive} />
        </td>
        <td className="px-4 py-3">
          <div className="flex gap-1">
            <button onClick={onTest} className="p-1 hover:bg-muted rounded text-primary cursor-pointer" title="Test">
              <Play className="w-4 h-4" />
            </button>
            <button onClick={onEdit} className="p-1 hover:bg-muted rounded cursor-pointer" title="Edit">
              <Pencil className="w-4 h-4" />
            </button>
            <button onClick={onDelete} className="p-1 hover:bg-muted rounded text-destructive cursor-pointer" title="Delete">
              <Trash2 className="w-4 h-4" />
            </button>
          </div>
        </td>
      </tr>
      {expanded && (
        <tr>
          <td colSpan={8} className="px-8 py-4 bg-muted/30">
            {detail.isLoading ? (
              <p className="text-sm text-muted-foreground">Loading steps...</p>
            ) : detail.data?.steps?.length ? (
              <div className="space-y-2">
                <p className="text-xs font-medium text-muted-foreground uppercase tracking-wide">Steps</p>
                <div className="grid gap-2">
                  {detail.data.steps.map((step) => (
                    <div key={step.id} className="flex items-center gap-4 text-xs bg-background border border-border rounded-md px-3 py-2">
                      <Badge variant="muted">{step.method}</Badge>
                      <span className="font-medium">{step.name}</span>
                      <span className="font-mono text-muted-foreground">{step.path_template}</span>
                      <span className="text-muted-foreground">
                        &rarr; {upstreamName(step.upstream_id)}
                      </span>
                      {step.depends_on?.length ? (
                        <span className="text-muted-foreground">
                          depends: {(step.depends_on as string[]).join(', ')}
                        </span>
                      ) : null}
                      <Badge variant={step.on_error === 'abort' ? 'destructive' : 'muted'}>
                        {step.on_error}
                      </Badge>
                    </div>
                  ))}
                </div>
                <div className="mt-2">
                  <p className="text-xs font-medium text-muted-foreground uppercase tracking-wide mb-1">Response Merge</p>
                  <pre className="text-xs font-mono bg-background border border-border rounded-md p-2 overflow-x-auto">
                    {JSON.stringify(comp.response_merge, null, 2)}
                  </pre>
                </div>
              </div>
            ) : (
              <p className="text-sm text-muted-foreground">No steps configured.</p>
            )}
          </td>
        </tr>
      )}
    </>
  );
}
