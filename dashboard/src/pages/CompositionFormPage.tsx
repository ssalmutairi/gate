import { useState, useEffect } from 'react';
import { useNavigate, useParams } from 'react-router-dom';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import {
  getUpstreams,
  getRoutes,
  getComposition,
  createComposition,
  updateComposition,
  getServices,
  getServiceSpec,
  getCompositionOpenApi,
  getCompositionNamespaces,
} from '../lib/api';
import { Button } from '../components/ui/button';
import { Input } from '../components/ui/input';
import { Label } from '../components/ui/label';
import { toast } from 'sonner';
import { ArrowLeft, Save, Settings, Play, Layers, FileOutput, FileJson, PanelRightClose, PanelRightOpen } from 'lucide-react';
import { FlowCanvas } from '../components/flow/FlowCanvas';
import { StepInspector } from '../components/flow/StepInspector';
import { emptyStep, type StepForm, type ServiceEndpoint } from '../components/flow/flowTypes';
import { ResponseMergeBuilder } from '../components/flow/ResponseMergeBuilder';
import { TestEndpointPanel, type TestResult } from '../components/flow/TestEndpointPanel';

const ALL_METHODS = ['GET', 'POST', 'PUT', 'PATCH', 'DELETE'];

/** Build a sample JSON body from step body_templates, using spec types for proper defaults. */
function buildSampleBody(steps: StepForm[], serviceEndpoints: Map<string, ServiceEndpoint[]>): string {
  if (!steps.length) return '';
  const fieldTypes = new Map<string, string>();
  for (const step of steps) {
    if (!step.body_template?.trim()) continue;
    let tpl: Record<string, string>;
    try { tpl = JSON.parse(step.body_template); } catch { continue; }
    const specTypeMap = new Map<string, string>();
    const eps = serviceEndpoints.get(step.upstream_id) ?? [];
    for (const ep of eps) {
      if (!ep.requestBodyProperties) continue;
      for (const prop of ep.requestBodyProperties) {
        if (prop.type) specTypeMap.set(prop.name, prop.type);
      }
    }
    for (const [specProp, val] of Object.entries(tpl)) {
      if (typeof val !== 'string') continue;
      const match = val.match(/^\$\{request\.body\.(\w+)\}$/);
      if (!match) continue;
      const reqField = match[1];
      const type = specTypeMap.get(specProp);
      if (type && !fieldTypes.has(reqField)) fieldTypes.set(reqField, type);
      else if (!fieldTypes.has(reqField)) fieldTypes.set(reqField, 'string');
    }
  }
  if (fieldTypes.size === 0) return '';
  const obj: Record<string, any> = {};
  for (const [name, type] of fieldTypes) {
    if (type === 'integer' || type === 'number' || type === 'int' || type === 's:int') obj[name] = 0;
    else if (type === 'boolean') obj[name] = false;
    else obj[name] = '';
  }
  return JSON.stringify(obj, null, 2);
}

/** Auto-generate an input JSON Schema by introspecting step body_templates for ${request.body.X} references. */
function autoGenerateInputSchema(steps: StepForm[], serviceEndpoints: Map<string, ServiceEndpoint[]>): any {
  const properties: Record<string, any> = {};
  const required: string[] = [];
  for (const step of steps) {
    if (!step.body_template?.trim()) continue;
    let tpl: Record<string, any>;
    try { tpl = JSON.parse(step.body_template); } catch { continue; }
    const specTypeMap = new Map<string, string>();
    const eps = serviceEndpoints.get(step.upstream_id) ?? [];
    for (const ep of eps) {
      if (!ep.requestBodyProperties) continue;
      for (const prop of ep.requestBodyProperties) {
        if (prop.type) specTypeMap.set(prop.name, prop.type);
      }
    }
    for (const [specProp, val] of Object.entries(tpl)) {
      if (typeof val !== 'string') continue;
      const match = val.match(/^\$\{request\.body\.(\w+)\}$/);
      if (!match) continue;
      const reqField = match[1];
      if (properties[reqField]) continue;
      const specType = specTypeMap.get(specProp);
      let jsonType = 'string';
      if (specType === 'integer' || specType === 'int' || specType === 's:int') jsonType = 'integer';
      else if (specType === 'number') jsonType = 'number';
      else if (specType === 'boolean') jsonType = 'boolean';
      properties[reqField] = { type: jsonType };
      required.push(reqField);
    }
  }
  const schema: any = { type: 'object', properties };
  if (required.length > 0) schema.required = required;
  return schema;
}

/** Auto-generate an output JSON Schema from the response_merge template. */
function autoGenerateOutputSchema(responseMerge: string): any {
  let parsed: any;
  try { parsed = JSON.parse(responseMerge); } catch { return { type: 'object' }; }
  if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) return { type: 'object' };
  const properties: Record<string, any> = {};
  for (const key of Object.keys(parsed)) {
    properties[key] = {}; // unknown type — user can refine
  }
  return { type: 'object', properties };
}

type InspectorTab = 'step' | 'response' | 'schema' | 'test' | 'settings';

export default function CompositionFormPage() {
  const navigate = useNavigate();
  const { id } = useParams<{ id: string; step: string }>();
  const isEditing = !!id;
  const qc = useQueryClient();

  const upstreams = useQuery({ queryKey: ['upstreams'], queryFn: getUpstreams });
  const routes = useQuery({ queryKey: ['routes'], queryFn: getRoutes });
  const services = useQuery({ queryKey: ['services'], queryFn: () => getServices() });
  const existing = useQuery({
    queryKey: ['composition', id],
    queryFn: () => getComposition(id!),
    enabled: isEditing,
  });
  const namespaces = useQuery({ queryKey: ['composition-namespaces'], queryFn: getCompositionNamespaces });

  // Service spec endpoints
  const [serviceEndpoints, setServiceEndpoints] = useState<Map<string, ServiceEndpoint[]>>(new Map());
  useEffect(() => {
    if (!services.data?.length) return;
    const map = new Map<string, ServiceEndpoint[]>();

    function resolveRef(spec: any, ref: string): any {
      if (!ref.startsWith('#/')) return {};
      const parts = ref.slice(2).split('/');
      let obj = spec;
      for (const p of parts) { obj = obj?.[p]; if (!obj) return {}; }
      return obj;
    }

    function schemaProperties(spec: any, schema: any): { name: string; required?: boolean; type?: string }[] {
      if (!schema) return [];
      const resolved = schema.$ref ? resolveRef(spec, schema.$ref) : schema;
      const props = resolved?.properties;
      if (!props) return [];
      const reqSet = new Set<string>(resolved.required ?? []);
      return Object.entries(props).map(([name, val]: [string, any]) => ({
        name, required: reqSet.has(name), type: val?.type ?? (val?.$ref ? 'object' : undefined),
      }));
    }

    Promise.all(
      services.data.map(async (svc) => {
        try {
          const spec = await getServiceSpec(svc.id);
          const endpoints: ServiceEndpoint[] = [];
          const serverUrl = spec.servers?.[0]?.url ?? '';
          const basePath = serverUrl.startsWith('/') ? serverUrl.replace(/\/$/, '') : '';
          if (spec.paths) {
            for (const [path, methods] of Object.entries(spec.paths as Record<string, any>)) {
              for (const [method, detail] of Object.entries(methods as Record<string, any>)) {
                if (!['get', 'post', 'put', 'patch', 'delete'].includes(method)) continue;
                const op = detail as any;
                const parameters = (op.parameters ?? []).map((p: any) => ({
                  name: p.name, in: p.in, required: p.required, schema: p.schema,
                }));
                const bodySchema = op.requestBody?.content?.['application/json']?.schema
                  ?? op.requestBody?.content?.['application/xml']?.schema;
                const requestBodyProperties = schemaProperties(spec, bodySchema);
                const resSchema = (op.responses?.['200'] ?? op.responses?.['201'])
                  ?.content?.['application/json']?.schema
                  ?? (op.responses?.['200'] ?? op.responses?.['201'])
                  ?.content?.['application/xml']?.schema;
                const resSolved = resSchema?.$ref ? resolveRef(spec, resSchema.$ref) : resSchema;
                const resObj = resSolved?.type === 'array' && resSolved?.items
                  ? (resSolved.items.$ref ? resolveRef(spec, resSolved.items.$ref) : resSolved.items)
                  : resSolved;
                const responseProperties = resObj?.properties ? Object.keys(resObj.properties) : [];
                const responseSchema: Record<string, string> = {};
                if (resObj?.properties) {
                  for (const [pName, pSchema] of Object.entries(resObj.properties as Record<string, any>)) {
                    responseSchema[pName] = pSchema?.type ?? 'object';
                  }
                }
                endpoints.push({
                  method: method.toUpperCase(), path,
                  fullPath: basePath + path, summary: op.summary,
                  parameters: parameters.length ? parameters : undefined,
                  requestBodyProperties: requestBodyProperties.length ? requestBodyProperties : undefined,
                  responseProperties: responseProperties.length ? responseProperties : undefined,
                  responseSchema: Object.keys(responseSchema).length ? responseSchema : undefined,
                });
              }
            }
          }
          if (endpoints.length) map.set(svc.upstream_id, endpoints);
        } catch { /* skip */ }
      })
    ).then(() => setServiceEndpoints(new Map(map)));
  }, [services.data]);

  // Form state
  const [name, setName] = useState('');
  const [pathPrefix, setPathPrefix] = useState('');
  const [pathPattern, setPathPattern] = useState('');
  const [methods, setMethods] = useState<string[]>([]);
  const [timeoutMs, setTimeoutMs] = useState(30000);
  const [maxWaitMs, setMaxWaitMs] = useState('60000');
  const [authSkip, setAuthSkip] = useState(false);
  const [namespace, setNamespace] = useState('');
  const [responseMerge, setResponseMerge] = useState('{}');
  const [inputSchema, setInputSchema] = useState('');
  const [outputSchema, setOutputSchema] = useState('');
  const [steps, setSteps] = useState<StepForm[]>([]);
  const [loaded, setLoaded] = useState(false);

  // Inspector state
  const [inspectorTab, setInspectorTab] = useState<InspectorTab>('settings');
  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);
  const [selectedStepIndex, setSelectedStepIndex] = useState<number>(-1);
  const [inspectorOpen, setInspectorOpen] = useState(true);

  // Test state
  const [testBody, setTestBody] = useState('');
  const [testResult, setTestResult] = useState<TestResult | null>(null);
  const [testBodyInit, setTestBodyInit] = useState(false);

  const defaultUpstreamId = upstreams.data?.[0]?.id ?? '';

  // Auto-generate test body
  useEffect(() => {
    if (!loaded || testBodyInit) return;
    const awaitingSpecs = (services.data?.length ?? 0) > 0 && serviceEndpoints.size === 0;
    if (awaitingSpecs) return;
    const sample = buildSampleBody(steps, serviceEndpoints);
    if (sample) { setTestBody(sample); setTestBodyInit(true); }
  }, [steps, serviceEndpoints, loaded, testBodyInit, services.data]);

  // Init new composition
  useEffect(() => {
    if (!isEditing && defaultUpstreamId && !loaded) {
      setSteps([emptyStep(defaultUpstreamId)]);
      setLoaded(true);
    }
  }, [isEditing, defaultUpstreamId, loaded]);

  // Load existing composition
  useEffect(() => {
    if (!isEditing || !existing.data || loaded) return;
    const full = existing.data;
    setName(full.name);
    setPathPrefix(full.path_prefix);
    setPathPattern(full.path_pattern ?? '');
    setMethods(full.methods ?? []);
    setNamespace(full.namespace ?? '');
    setTimeoutMs(full.timeout_ms);
    setMaxWaitMs(full.max_wait_ms?.toString() ?? '');
    setAuthSkip(full.auth_skip);
    setResponseMerge(JSON.stringify(full.response_merge, null, 2));
    setInputSchema(full.input_schema ? JSON.stringify(full.input_schema, null, 2) : '');
    setOutputSchema(full.output_schema ? JSON.stringify(full.output_schema, null, 2) : '');
    setSteps(
      (full.steps ?? []).map((s) => ({
        name: s.name, method: s.method, upstream_id: s.upstream_id,
        path_template: s.path_template, depends_on: s.depends_on ?? [],
        on_error: s.on_error,
        default_value: s.default_value ? JSON.stringify(s.default_value) : '',
        timeout_ms: s.timeout_ms,
        body_template: s.body_template ? JSON.stringify(s.body_template, null, 2) : '',
        use_internal_route: s.use_internal_route ?? false,
      }))
    );
    setLoaded(true);
  }, [isEditing, existing.data, loaded]);

  const [savedId, setSavedId] = useState<string | null>(id ?? null);

  const createMut = useMutation({
    mutationFn: createComposition,
    onSuccess: (data) => {
      qc.invalidateQueries({ queryKey: ['compositions'] });
      if (data?.id) setSavedId(data.id);
    },
    onError: (e: any) => toast.error(e.response?.data?.error ?? 'Failed to create composition'),
  });

  const updateMut = useMutation({
    mutationFn: ({ compId, data }: { compId: string; data: any }) => updateComposition(compId, data),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['compositions'] });
      qc.invalidateQueries({ queryKey: ['composition', savedId ?? id] });
    },
    onError: (e: any) => toast.error(e.response?.data?.error ?? 'Failed to update composition'),
  });

  const buildPayload = () => {
    let parsedMerge: any;
    try { parsedMerge = JSON.parse(responseMerge); } catch {
      toast.error('Invalid JSON in response merge template');
      return null;
    }
    const builtSteps = steps.map((s) => {
      const step: any = {
        name: s.name, method: s.method, upstream_id: s.upstream_id,
        path_template: s.path_template, on_error: s.on_error, timeout_ms: s.timeout_ms,
        use_internal_route: s.use_internal_route
          || (services.data?.some((svc) => svc.upstream_id === s.upstream_id) ?? false)
          || (routes.data?.some((r) => r.upstream_id === s.upstream_id && r.active) ?? false),
      };
      if (s.depends_on.length > 0) step.depends_on = s.depends_on;
      if (s.body_template.trim()) {
        try { step.body_template = JSON.parse(s.body_template); } catch {
          toast.error(`Invalid JSON in body template for step "${s.name}"`); return null;
        }
      }
      if (s.default_value.trim()) {
        try { step.default_value = JSON.parse(s.default_value); } catch {
          toast.error(`Invalid JSON in default value for step "${s.name}"`); return null;
        }
      }
      return step;
    });
    if (builtSteps.some((s) => s === null)) return null;
    let parsedInputSchema: any = undefined;
    if (inputSchema.trim()) {
      try { parsedInputSchema = JSON.parse(inputSchema); } catch {
        toast.error('Invalid JSON in input schema');
        return null;
      }
    }
    let parsedOutputSchema: any = undefined;
    if (outputSchema.trim()) {
      try { parsedOutputSchema = JSON.parse(outputSchema); } catch {
        toast.error('Invalid JSON in output schema');
        return null;
      }
    }
    return {
      name, path_prefix: pathPrefix, path_pattern: pathPattern || undefined,
      methods: methods.length > 0 ? methods : undefined,
      namespace: namespace.trim() || undefined,
      timeout_ms: timeoutMs, max_wait_ms: maxWaitMs ? parseInt(maxWaitMs) : undefined,
      auth_skip: authSkip, response_merge: parsedMerge,
      input_schema: parsedInputSchema, output_schema: parsedOutputSchema,
      steps: builtSteps,
    };
  };

  const handleSave = () => {
    const payload = buildPayload();
    if (!payload) return;
    if (isEditing || savedId) {
      updateMut.mutate({ compId: (savedId ?? id)!, data: payload }, {
        onSuccess: () => toast.success('Composition saved'),
      });
    } else {
      createMut.mutate(payload, {
        onSuccess: (data) => {
          toast.success('Composition created');
          if (data?.id) navigate(`/compositions/${data.id}/edit`, { replace: true });
        },
      });
    }
  };

  const handleSaveForTest = async (): Promise<boolean> => {
    const payload = buildPayload();
    if (!payload) return false;
    try {
      if (savedId) {
        await updateMut.mutateAsync({ compId: savedId, data: payload });
      } else {
        const data = await createMut.mutateAsync(payload);
        if (data?.id) setSavedId(data.id);
      }
      toast.success('Composition saved for testing');
      return true;
    } catch { return false; }
  };

  const toggleMethod = (method: string) => {
    setMethods((prev) => prev.includes(method) ? prev.filter((m) => m !== method) : [...prev, method]);
  };

  const isPending = createMut.isPending || updateMut.isPending;
  const canSave = name.trim() !== '' && pathPrefix.trim() !== '' && steps.length > 0 && steps.every((s) => s.name && s.upstream_id);

  // Handle node selection from canvas
  const handleNodeSelect = (nodeId: string | null) => {
    setSelectedNodeId(nodeId);
    if (nodeId) {
      // Find the step index by looking up in the canvas nodes
      const canvas = (window as any).__flowCanvas;
      if (canvas?.nodes?.current) {
        const idx = canvas.nodes.current.findIndex((n: any) => n.id === nodeId);
        setSelectedStepIndex(idx);
      }
      setInspectorTab('step');
      setInspectorOpen(true);
    } else {
      setSelectedStepIndex(-1);
    }
  };

  // Derive selected step from the steps array (source of truth)
  const selectedStep = selectedStepIndex >= 0 && selectedStepIndex < steps.length
    ? { step: steps[selectedStepIndex], index: selectedStepIndex + 1 }
    : null;

  const handleStepUpdate = (field: keyof StepForm, value: any) => {
    const canvas = (window as any).__flowCanvas;
    if (canvas && selectedNodeId) {
      canvas.updateNodeData(selectedNodeId, field, value);
    }
  };

  const handleStepDelete = () => {
    const canvas = (window as any).__flowCanvas;
    if (canvas && selectedNodeId) {
      canvas.deleteNode(selectedNodeId);
      setSelectedNodeId(null);
      setSelectedStepIndex(-1);
    }
  };

  if (isEditing && existing.isLoading) {
    return (
      <div className="flex items-center justify-center h-64 text-muted-foreground">
        Loading composition...
      </div>
    );
  }

  if (isEditing && existing.isError) {
    return (
      <div className="flex flex-col items-center justify-center h-64 gap-4">
        <p className="text-destructive">Failed to load composition</p>
        <Button variant="secondary" onClick={() => navigate('/compositions')}>
          <ArrowLeft className="w-4 h-4 mr-1" /> Back to Compositions
        </Button>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-[calc(100vh-3.5rem-3rem)]">
      {/* Top bar */}
      <div className="flex items-center justify-between px-1 pb-2 shrink-0">
        <div className="flex items-center gap-3">
          <Button variant="ghost" size="sm" onClick={() => navigate('/compositions')}>
            <ArrowLeft className="w-4 h-4" />
          </Button>
          <input
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="Composition name..."
            className="text-lg font-bold bg-transparent border-none outline-none focus:ring-0 w-64 placeholder:text-muted-foreground/50"
          />
          {pathPrefix && (
            <span className="text-xs font-mono text-muted-foreground bg-muted px-2 py-0.5 rounded">
              {pathPrefix}{pathPattern}
            </span>
          )}
        </div>

        <div className="flex items-center gap-2">
          {/* Inspector tab buttons */}
          <div className="flex items-center border border-border rounded-md overflow-hidden">
            <button
              type="button"
              onClick={() => { setInspectorTab('settings'); setInspectorOpen(true); setSelectedNodeId(null); }}
              className={`px-2.5 py-1.5 text-xs flex items-center gap-1 cursor-pointer transition-colors ${
                inspectorTab === 'settings' && inspectorOpen ? 'bg-primary text-primary-foreground' : 'hover:bg-muted'
              }`}
              title="Settings"
            >
              <Settings className="w-3.5 h-3.5" />
            </button>
            <button
              type="button"
              onClick={() => { setInspectorTab('response'); setInspectorOpen(true); setSelectedNodeId(null); }}
              className={`px-2.5 py-1.5 text-xs flex items-center gap-1 cursor-pointer transition-colors border-l border-border ${
                inspectorTab === 'response' && inspectorOpen ? 'bg-primary text-primary-foreground' : 'hover:bg-muted'
              }`}
              title="Response Transform"
            >
              <FileOutput className="w-3.5 h-3.5" />
            </button>
            <button
              type="button"
              onClick={() => { setInspectorTab('schema'); setInspectorOpen(true); setSelectedNodeId(null); }}
              className={`px-2.5 py-1.5 text-xs flex items-center gap-1 cursor-pointer transition-colors border-l border-border ${
                inspectorTab === 'schema' && inspectorOpen ? 'bg-primary text-primary-foreground' : 'hover:bg-muted'
              }`}
              title="Schema"
            >
              <FileJson className="w-3.5 h-3.5" />
            </button>
            <button
              type="button"
              onClick={() => { setInspectorTab('test'); setInspectorOpen(true); setSelectedNodeId(null); }}
              className={`px-2.5 py-1.5 text-xs flex items-center gap-1 cursor-pointer transition-colors border-l border-border ${
                inspectorTab === 'test' && inspectorOpen ? 'bg-primary text-primary-foreground' : 'hover:bg-muted'
              }`}
              title="Test"
            >
              <Play className="w-3.5 h-3.5" />
            </button>
          </div>

          <button
            type="button"
            onClick={() => setInspectorOpen(!inspectorOpen)}
            className="p-1.5 hover:bg-muted rounded cursor-pointer"
            title={inspectorOpen ? 'Hide panel' : 'Show panel'}
          >
            {inspectorOpen ? <PanelRightClose className="w-4 h-4" /> : <PanelRightOpen className="w-4 h-4" />}
          </button>

          <Button type="button" onClick={handleSave} disabled={isPending || !canSave} size="sm">
            <Save className="w-3.5 h-3.5 mr-1" />
            {isPending ? 'Saving...' : 'Save'}
          </Button>
        </div>
      </div>

      {/* Main IDE area */}
      <div className="flex-1 min-h-0 flex gap-0">
        {/* Canvas — center */}
        <div className="flex-1 min-w-0 border border-border rounded-l-md overflow-hidden">
          {loaded && (
            <FlowCanvas
              initialSteps={steps}
              upstreams={upstreams.data ?? []}
              routes={routes.data ?? []}
              services={services.data ?? []}
              serviceEndpoints={serviceEndpoints}
              onChange={setSteps}
              onNodeSelect={handleNodeSelect}
              selectedNodeId={selectedNodeId}
            />
          )}
        </div>

        {/* Inspector — right panel */}
        {inspectorOpen && (
          <div className="w-[340px] shrink-0 border border-l-0 border-border rounded-r-md bg-card overflow-y-auto">
            <div className="p-4">
              {/* Step inspector — when a node is selected */}
              {inspectorTab === 'step' && selectedStep ? (
                <StepInspector
                  key={selectedNodeId}
                  step={selectedStep.step}
                  stepIndex={selectedStep.index}
                  upstreams={upstreams.data ?? []}
                  routes={routes.data ?? []}
                  services={services.data ?? []}
                  serviceEndpoints={serviceEndpoints}
                  onUpdate={handleStepUpdate}
                  onDelete={handleStepDelete}
                />
              ) : inspectorTab === 'step' && !selectedStep ? (
                <div className="text-center text-sm text-muted-foreground py-12">
                  <Layers className="w-8 h-8 mx-auto mb-3 opacity-50" />
                  <p>Select a step on the canvas</p>
                  <p className="text-xs mt-1">or click "Add Step" to create one</p>
                </div>
              ) : null}

              {/* Settings panel */}
              {inspectorTab === 'settings' && (
                <div className="space-y-4">
                  <h3 className="text-sm font-semibold">Composition Settings</h3>

                  <div className="space-y-1.5">
                    <Label className="text-xs">Name</Label>
                    <Input value={name} onChange={(e) => setName(e.target.value)} placeholder="my-composition" className="h-8 text-xs" />
                  </div>

                  <div className="space-y-1.5">
                    <Label className="text-xs">Path Prefix</Label>
                    <Input value={pathPrefix} onChange={(e) => setPathPrefix(e.target.value)} placeholder="/user-profile" className="h-8 text-xs" />
                  </div>

                  <div className="space-y-1.5">
                    <Label className="text-xs">Path Pattern</Label>
                    <Input value={pathPattern} onChange={(e) => setPathPattern(e.target.value)} placeholder="/:id (optional)" className="h-8 text-xs" />
                  </div>

                  <div className="space-y-1.5">
                    <Label className="text-xs">Namespace</Label>
                    <Input
                      value={namespace}
                      onChange={(e) => setNamespace(e.target.value)}
                      placeholder="e.g. user-management (optional)"
                      className="h-8 text-xs"
                      list="namespace-suggestions"
                    />
                    <datalist id="namespace-suggestions">
                      {namespaces.data
                        ?.filter((ns) => ns.namespace !== null)
                        .map((ns) => (
                          <option key={ns.namespace!} value={ns.namespace!} />
                        ))}
                    </datalist>
                  </div>

                  <div className="space-y-1.5">
                    <Label className="text-xs">Methods</Label>
                    <div className="flex items-center gap-1.5 flex-wrap">
                      {ALL_METHODS.map((m) => (
                        <button
                          key={m}
                          type="button"
                          onClick={() => toggleMethod(m)}
                          className={`px-2 py-0.5 text-xs rounded border cursor-pointer ${
                            methods.includes(m)
                              ? 'bg-primary text-primary-foreground border-primary'
                              : 'border-border hover:bg-muted'
                          }`}
                        >
                          {m}
                        </button>
                      ))}
                    </div>
                    {methods.length === 0 && <span className="text-[10px] text-muted-foreground">(all methods)</span>}
                  </div>

                  <div className="grid grid-cols-2 gap-2">
                    <div className="space-y-1.5">
                      <Label className="text-xs">Timeout (ms)</Label>
                      <Input type="number" value={timeoutMs} onChange={(e) => setTimeoutMs(parseInt(e.target.value) || 30000)} className="h-8 text-xs" />
                    </div>
                    <div className="space-y-1.5">
                      <Label className="text-xs">Max Wait (ms)</Label>
                      <Input value={maxWaitMs} onChange={(e) => setMaxWaitMs(e.target.value)} placeholder="optional" className="h-8 text-xs" />
                    </div>
                  </div>

                  <label className="flex items-center gap-2 text-xs cursor-pointer">
                    <input type="checkbox" checked={authSkip} onChange={(e) => setAuthSkip(e.target.checked)} className="rounded border-border" />
                    <span>Skip authentication</span>
                  </label>
                </div>
              )}

              {/* Response merge panel */}
              {inspectorTab === 'response' && (
                <ResponseMergeBuilder
                  value={responseMerge}
                  onChange={setResponseMerge}
                  steps={steps}
                  serviceEndpoints={serviceEndpoints}
                />
              )}

              {/* Schema panel */}
              {inspectorTab === 'schema' && (
                <div className="space-y-4">
                  <h3 className="text-sm font-semibold">Input / Output Schema</h3>

                  <div className="space-y-1.5">
                    <div className="flex items-center justify-between">
                      <Label className="text-xs">Input Schema (JSON Schema)</Label>
                      <button
                        type="button"
                        className="text-[10px] text-primary hover:underline cursor-pointer"
                        onClick={() => {
                          const schema = autoGenerateInputSchema(steps, serviceEndpoints);
                          setInputSchema(JSON.stringify(schema, null, 2));
                          toast.success('Input schema generated from steps');
                        }}
                      >
                        Auto-generate
                      </button>
                    </div>
                    <textarea
                      value={inputSchema}
                      onChange={(e) => setInputSchema(e.target.value)}
                      placeholder='{"type":"object","properties":{...},"required":[...]}'
                      className="w-full h-32 px-3 py-2 text-xs font-mono border border-border rounded-md bg-background resize-y"
                    />
                  </div>

                  <div className="space-y-1.5">
                    <div className="flex items-center justify-between">
                      <Label className="text-xs">Output Schema (JSON Schema)</Label>
                      <button
                        type="button"
                        className="text-[10px] text-primary hover:underline cursor-pointer"
                        onClick={() => {
                          const schema = autoGenerateOutputSchema(responseMerge);
                          setOutputSchema(JSON.stringify(schema, null, 2));
                          toast.success('Output schema generated from response transform');
                        }}
                      >
                        Auto-generate
                      </button>
                    </div>
                    <textarea
                      value={outputSchema}
                      onChange={(e) => setOutputSchema(e.target.value)}
                      placeholder='{"type":"object","properties":{...}}'
                      className="w-full h-32 px-3 py-2 text-xs font-mono border border-border rounded-md bg-background resize-y"
                    />
                  </div>

                  {(savedId || id) && (
                    <Button
                      type="button"
                      variant="secondary"
                      size="sm"
                      className="w-full"
                      onClick={async () => {
                        try {
                          const spec = await getCompositionOpenApi((savedId ?? id)!);
                          const blob = new Blob([JSON.stringify(spec, null, 2)], { type: 'application/json' });
                          const url = URL.createObjectURL(blob);
                          window.open(url, '_blank');
                        } catch {
                          toast.error('Save the composition first to view OpenAPI spec');
                        }
                      }}
                    >
                      <FileJson className="w-3.5 h-3.5 mr-1" />
                      View OpenAPI Spec
                    </Button>
                  )}
                </div>
              )}

              {/* Test panel */}
              {inspectorTab === 'test' && (
                <div className="space-y-3">
                  <h3 className="text-sm font-semibold">Test Endpoint</h3>
                  {!pathPrefix ? (
                    <p className="text-xs text-muted-foreground">Set a path prefix in Settings first.</p>
                  ) : (
                    <TestEndpointPanel
                      pathPrefix={pathPrefix}
                      pathPattern={pathPattern}
                      methods={methods}
                      defaultOpen
                      compact
                      onSaveBeforeTest={handleSaveForTest}
                      requestBody={testBody}
                      onRequestBodyChange={setTestBody}
                      lastResult={testResult}
                      onResultChange={setTestResult}
                      inputSchema={inputSchema.trim() ? (() => { try { return JSON.parse(inputSchema); } catch { return undefined; } })() : undefined}
                    />
                  )}
                </div>
              )}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
