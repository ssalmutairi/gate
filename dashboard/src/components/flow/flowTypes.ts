import type { Upstream, Route, Service } from '../../lib/api';

/** An endpoint extracted from a service's OpenAPI/WSDL spec. */
export interface ServiceEndpoint {
  method: string;
  path: string;
  /** Full path including the server base path (e.g. /api/v3/pet). */
  fullPath: string;
  summary?: string;
  /** Path/query parameters from the spec. */
  parameters?: { name: string; in: string; required?: boolean; schema?: any }[];
  /** Properties from the request body schema (flattened). */
  requestBodyProperties?: { name: string; required?: boolean; type?: string }[];
  /** Properties from the response schema (flattened). */
  responseProperties?: string[];
  /** Response schema properties with types (e.g. { AddResult: "integer" }). */
  responseSchema?: Record<string, string>;
}

export const NODE_WIDTH = 200;
export const NODE_HEIGHT = 65;

export const ALL_METHODS = ['GET', 'POST', 'PUT', 'PATCH', 'DELETE'];
export const ERROR_POLICIES = ['abort', 'skip', 'default'];

export interface StepForm {
  name: string;
  method: string;
  upstream_id: string;
  path_template: string;
  depends_on: string[];
  on_error: string;
  default_value: string;
  timeout_ms: number;
  body_template: string;
  use_internal_route: boolean;
}

export interface StepNodeData extends StepForm {
  upstreams: Upstream[];
  routes: Route[];
  services: Service[];
  serviceEndpoints: Map<string, ServiceEndpoint[]>; // upstream_id → endpoints
  allNodeNames: string[];
  stepIndex: number;
  badgeColor: string;
  onUpdate: (field: keyof StepForm, value: any) => void;
  onDelete: () => void;
  onFocusNode: () => void;
}

/** Compact display-only node data for the IDE layout. */
export interface CompactNodeData extends StepForm {
  stepIndex: number;
  badgeColor: string;
  upstreamName: string;
  onSelect: () => void;
}

/**
 * Palette of distinct badge colors.
 * Nodes in the same dependency group (parallel) share a color.
 */
export const BADGE_COLORS = [
  '#6366f1', // indigo
  '#f59e0b', // amber
  '#10b981', // emerald
  '#ef4444', // red
  '#8b5cf6', // violet
  '#ec4899', // pink
  '#14b8a6', // teal
  '#f97316', // orange
  '#3b82f6', // blue
  '#84cc16', // lime
];

export const emptyStep = (upstreamId: string): StepForm => ({
  name: '',
  method: 'GET',
  upstream_id: upstreamId,
  path_template: '',
  depends_on: [],
  on_error: 'abort',
  default_value: '',
  timeout_ms: 10000,
  body_template: '',
  use_internal_route: true,
});
