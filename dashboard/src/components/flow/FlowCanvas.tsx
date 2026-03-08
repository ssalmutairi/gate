import { useCallback, useEffect, useRef, useState } from 'react';
import {
  ReactFlow,
  useNodesState,
  useEdgesState,
  useReactFlow,
  ReactFlowProvider,
  addEdge,
  Panel,
  type Connection,
  type Edge,
  type Node,
  type ColorMode,
} from '@xyflow/react';
import '@xyflow/react/dist/style.css';
import { CompactNode } from './CompactNode';
import { layoutGraph } from './layoutGraph';
import { emptyStep, BADGE_COLORS, type StepForm, type CompactNodeData } from './flowTypes';
import type { Upstream, Route, Service } from '../../lib/api';
import type { ServiceEndpoint } from './flowTypes';
import { useTheme } from '../../hooks/useTheme';
import { THEMES } from '../../lib/themes';
import { Button } from '../ui/button';
import { Plus, LayoutGrid } from 'lucide-react';
import { toast } from 'sonner';

const nodeTypes = { compact: CompactNode };

interface FlowCanvasProps {
  initialSteps: StepForm[];
  upstreams: Upstream[];
  routes: Route[];
  services: Service[];
  serviceEndpoints: Map<string, ServiceEndpoint[]>;
  onChange: (steps: StepForm[]) => void;
  onNodeSelect?: (nodeId: string | null) => void;
  selectedNodeId?: string | null;
}

let nextId = 0;
function genId() {
  return `step-${++nextId}`;
}

function hasCycle(edges: Edge[], source: string, target: string): boolean {
  const adj = new Map<string, string[]>();
  for (const e of edges) {
    if (!adj.has(e.source)) adj.set(e.source, []);
    adj.get(e.source)!.push(e.target);
  }
  const visited = new Set<string>();
  const queue = [target];
  while (queue.length > 0) {
    const node = queue.shift()!;
    if (node === source) return true;
    if (visited.has(node)) continue;
    visited.add(node);
    for (const next of adj.get(node) ?? []) {
      queue.push(next);
    }
  }
  return false;
}

function computeBadgeColors(nodes: Node[], edges: Edge[]): Map<string, string> {
  const colors = new Map<string, string>();
  const groupToColor = new Map<string, string>();
  let colorIdx = 0;
  for (const node of nodes) {
    const deps = edges
      .filter((e) => e.target === node.id)
      .map((e) => e.source)
      .sort()
      .join(',');
    if (!groupToColor.has(deps)) {
      groupToColor.set(deps, BADGE_COLORS[colorIdx % BADGE_COLORS.length]);
      colorIdx++;
    }
    colors.set(node.id, groupToColor.get(deps)!);
  }
  return colors;
}

function nodesToSteps(nodes: Node<CompactNodeData>[], edges: Edge[]): StepForm[] {
  return nodes.map((node) => {
    const d = node.data;
    const dependsOn = edges
      .filter((e) => e.target === node.id)
      .map((e) => nodes.find((n) => n.id === e.source)?.data.name)
      .filter(Boolean) as string[];
    return {
      name: d.name,
      method: d.method,
      upstream_id: d.upstream_id,
      path_template: d.path_template,
      depends_on: dependsOn,
      on_error: d.on_error,
      default_value: d.default_value,
      timeout_ms: d.timeout_ms,
      body_template: d.body_template,
      use_internal_route: d.use_internal_route,
    };
  });
}

/** Map from node ID → step index (1-based). Exported for parent to look up. */
export function getNodeStepIndex(nodes: Node[], nodeId: string): number {
  const idx = nodes.findIndex((n) => n.id === nodeId);
  return idx >= 0 ? idx + 1 : 0;
}

function FlowCanvasInner({
  initialSteps,
  upstreams,
  routes,
  services,
  serviceEndpoints,
  onChange,
  onNodeSelect,
  selectedNodeId,
}: FlowCanvasProps) {
  const { theme } = useTheme();
  const colorMode: ColorMode = THEMES[theme].isDark ? 'dark' : 'light';
  const { fitView } = useReactFlow();

  const [nodes, setNodes, onNodesChange] = useNodesState<Node<CompactNodeData>>([]);
  const [edges, setEdges, onEdgesChange] = useEdgesState<Edge>([]);

  const initialized = useRef(false);
  const nodesRef = useRef(nodes);
  const edgesRef = useRef(edges);
  const onChangeRef = useRef(onChange);
  const onNodeSelectRef = useRef(onNodeSelect);
  const upstreamsRef = useRef(upstreams);

  nodesRef.current = nodes;
  edgesRef.current = edges;
  onChangeRef.current = onChange;
  onNodeSelectRef.current = onNodeSelect;
  upstreamsRef.current = upstreams;

  const syncToParent = useCallback(() => {
    const steps = nodesToSteps(nodesRef.current, edgesRef.current);
    onChangeRef.current(steps);
  }, []);

  const refreshBadgeColors = useCallback(() => {
    const colors = computeBadgeColors(nodesRef.current, edgesRef.current);
    setNodes((prev) => {
      const next = prev.map((n) => ({
        ...n,
        data: { ...n.data, badgeColor: colors.get(n.id) ?? BADGE_COLORS[0] },
      }));
      nodesRef.current = next;
      return next;
    });
  }, [setNodes]);

  const upstreamName = useCallback((upstreamId: string) => {
    return upstreamsRef.current?.find((u) => u.id === upstreamId)?.name ?? 'Unknown';
  }, []);

  const makeNodeData = useCallback(
    (step: StepForm, nodeId: string, index: number, color?: string): CompactNodeData => ({
      ...step,
      stepIndex: index,
      badgeColor: color ?? BADGE_COLORS[0],
      upstreamName: upstreamName(step.upstream_id),
      onSelect: () => onNodeSelectRef.current?.(nodeId),
    }),
    [upstreamName]
  );

  // Initialize from steps on mount
  useEffect(() => {
    if (initialized.current) return;
    initialized.current = true;

    const nameToId = new Map<string, string>();
    const newNodes: Node<CompactNodeData>[] = initialSteps.map((step, i) => {
      const id = genId();
      nameToId.set(step.name, id);
      return {
        id,
        type: 'compact' as const,
        position: { x: 0, y: 0 },
        dragHandle: '.compact-node-header',
        data: makeNodeData(step, id, i + 1),
      };
    });

    const newEdges: Edge[] = [];
    for (let i = 0; i < initialSteps.length; i++) {
      for (const dep of initialSteps[i].depends_on) {
        const sourceId = nameToId.get(dep);
        if (sourceId) {
          newEdges.push({
            id: `e-${sourceId}-${newNodes[i].id}`,
            source: sourceId,
            target: newNodes[i].id,
            animated: true,
          });
        }
      }
    }

    const colors = computeBadgeColors(newNodes, newEdges);
    const colored = newNodes.map((n) => ({
      ...n,
      data: { ...n.data, badgeColor: colors.get(n.id) ?? BADGE_COLORS[0] },
    }));

    const laid = layoutGraph(colored, newEdges);
    nodesRef.current = laid as Node<CompactNodeData>[];
    edgesRef.current = newEdges;
    setNodes(laid as Node<CompactNodeData>[]);
    setEdges(newEdges);
  }, [initialSteps, makeNodeData, setNodes, setEdges]);

  // Update upstream names when upstreams change
  useEffect(() => {
    if (!initialized.current) return;
    setNodes((prev) =>
      prev.map((node) => ({
        ...node,
        data: { ...node.data, upstreamName: upstreamName(node.data.upstream_id) },
      }))
    );
  }, [upstreams, setNodes, upstreamName]);

  /** Update a specific node's data from the inspector panel. */
  const updateNodeData = useCallback(
    (nodeId: string, field: keyof StepForm, value: any) => {
      setNodes((prev) => {
        const next = prev.map((n) =>
          n.id === nodeId
            ? {
                ...n,
                data: {
                  ...n.data,
                  [field]: value,
                  ...(field === 'upstream_id' ? { upstreamName: upstreamName(value) } : {}),
                },
              }
            : n
        );
        nodesRef.current = next;
        return next;
      });
      setTimeout(() => syncToParent(), 0);
    },
    [setNodes, syncToParent, upstreamName]
  );

  /** Delete a node. */
  const deleteNode = useCallback(
    (nodeId: string) => {
      setNodes((prev) => {
        const next = prev.filter((n) => n.id !== nodeId);
        nodesRef.current = next;
        return next;
      });
      setEdges((prev) => {
        const next = prev.filter((e) => e.source !== nodeId && e.target !== nodeId);
        edgesRef.current = next;
        return next;
      });
      onNodeSelectRef.current?.(null);
      setTimeout(() => syncToParent(), 0);
    },
    [setNodes, setEdges, syncToParent]
  );

  const onConnect = useCallback(
    (connection: Connection) => {
      if (hasCycle(edgesRef.current, connection.source, connection.target)) {
        toast.warning('Cannot connect: this would create a cycle');
        return;
      }
      setEdges((prev) => {
        const next = addEdge({ ...connection, animated: true }, prev);
        edgesRef.current = next;
        return next;
      });
      setTimeout(() => {
        refreshBadgeColors();
        syncToParent();
      }, 0);
    },
    [setEdges, syncToParent, refreshBadgeColors]
  );

  const handleAdd = useCallback(() => {
    const defaultUpstreamId = upstreamsRef.current?.[0]?.id ?? '';
    const step = emptyStep(defaultUpstreamId);
    const id = genId();
    const nextIndex = nodesRef.current.length + 1;
    const newNode: Node<CompactNodeData> = {
      id,
      type: 'compact',
      position: { x: Math.random() * 200 + 50, y: Math.random() * 200 + 50 },
      dragHandle: '.compact-node-header',
      data: makeNodeData(step, id, nextIndex),
    };
    setNodes((prev) => {
      const next = [...prev, newNode];
      nodesRef.current = next;
      return next;
    });
    setTimeout(() => {
      syncToParent();
      onNodeSelectRef.current?.(id);
    }, 0);
  }, [setNodes, makeNodeData, syncToParent]);

  const handleAutoLayout = useCallback(() => {
    const laid = layoutGraph(nodesRef.current as Node[], edgesRef.current);
    nodesRef.current = laid as Node<CompactNodeData>[];
    setNodes(laid as Node<CompactNodeData>[]);
    requestAnimationFrame(() => {
      fitView({ padding: 0.15, duration: 300 });
    });
  }, [setNodes, fitView]);

  // Expose updateNodeData and deleteNode via a ref-based approach
  // by attaching to window (simpler than context for this use case)
  useEffect(() => {
    (window as any).__flowCanvas = { updateNodeData, deleteNode, nodes: nodesRef };
    return () => { delete (window as any).__flowCanvas; };
  }, [updateNodeData, deleteNode]);

  return (
    <div className="h-full">
      <ReactFlow
        nodes={nodes}
        edges={edges}
        onNodesChange={onNodesChange}
        onEdgesChange={(changes) => {
          onEdgesChange(changes);
          if (changes.some((c) => c.type === 'remove')) {
            setTimeout(() => {
              refreshBadgeColors();
              syncToParent();
            }, 0);
          }
        }}
        onConnect={onConnect}
        onPaneClick={() => onNodeSelect?.(null)}
        nodeTypes={nodeTypes}
        colorMode={colorMode}
        fitView
        fitViewOptions={{ padding: 0.3 }}
        deleteKeyCode={null}
      >
        <Panel position="top-right" className="flex gap-2">
          <Button type="button" variant="secondary" size="sm" onClick={handleAdd}>
            <Plus className="w-3 h-3 mr-1" /> Add Step
          </Button>
          <Button type="button" variant="secondary" size="sm" onClick={handleAutoLayout}>
            <LayoutGrid className="w-3 h-3 mr-1" /> Auto Layout
          </Button>
        </Panel>
        {nodes.length > 1 && (
          <Panel position="top-left" className="flex gap-1.5 items-center">
            <span className="text-[10px] text-muted-foreground mr-0.5">Steps:</span>
            {nodes.map((node) => (
              <button
                key={node.id}
                type="button"
                onClick={() => {
                  onNodeSelect?.(node.id);
                  fitView({ nodes: [{ id: node.id }], padding: 0.5, duration: 300 });
                }}
                style={{ backgroundColor: node.data.badgeColor }}
                className={`w-5 h-5 rounded-full text-white text-[10px] font-bold flex items-center justify-center shadow cursor-pointer hover:scale-110 transition-transform ${
                  selectedNodeId === node.id ? 'ring-2 ring-white' : ''
                }`}
                title={node.data.name || `Step ${node.data.stepIndex}`}
              >
                {node.data.stepIndex}
              </button>
            ))}
          </Panel>
        )}
      </ReactFlow>
    </div>
  );
}

export function FlowCanvas(props: FlowCanvasProps) {
  return (
    <ReactFlowProvider>
      <FlowCanvasInner {...props} />
    </ReactFlowProvider>
  );
}
