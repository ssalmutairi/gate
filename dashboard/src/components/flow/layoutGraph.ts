import dagre from 'dagre';
import type { Node, Edge } from '@xyflow/react';
import { NODE_WIDTH, NODE_HEIGHT } from './flowTypes';

/** Badge extends 12px above node (-top-3) — include in layout height. */
const BADGE_OVERFLOW = 12;

/**
 * Measure actual DOM height of a rendered node.
 * Uses the React Flow wrapper element, adds badge overflow.
 * Falls back to NODE_HEIGHT if the node isn't in the DOM yet.
 */
function measureNodeHeight(nodeId: string): number {
  const wrapper = document.querySelector<HTMLElement>(`[data-id="${nodeId}"]`);
  if (!wrapper) return NODE_HEIGHT;
  return Math.ceil(wrapper.getBoundingClientRect().height) + BADGE_OVERFLOW;
}

export function layoutGraph(nodes: Node[], edges: Edge[]): Node[] {
  const heights = new Map<string, number>();
  for (const node of nodes) {
    heights.set(node.id, measureNodeHeight(node.id));
  }

  // Use the tallest node in each rank to ensure uniform spacing
  const maxHeight = Math.max(...heights.values(), NODE_HEIGHT);

  const g = new dagre.graphlib.Graph();
  g.setDefaultEdgeLabel(() => ({}));
  g.setGraph({ rankdir: 'LR', nodesep: 30, ranksep: 80 });

  for (const node of nodes) {
    // Give dagre the max height so all nodes in a rank get equal spacing
    g.setNode(node.id, { width: NODE_WIDTH, height: maxHeight });
  }
  for (const edge of edges) {
    g.setEdge(edge.source, edge.target);
  }

  dagre.layout(g);

  return nodes.map((node) => {
    const pos = g.node(node.id);
    return {
      ...node,
      position: {
        x: pos.x - NODE_WIDTH / 2,
        y: pos.y - maxHeight / 2,
      },
    };
  });
}
