import { memo } from 'react';
import { Handle, Position, type NodeProps, type Node } from '@xyflow/react';
import type { CompactNodeData } from './flowTypes';

type CompactNodeType = Node<CompactNodeData, 'compact'>;

const METHOD_COLORS: Record<string, string> = {
  GET: 'bg-green-500/15 text-green-600 dark:text-green-400',
  POST: 'bg-blue-500/15 text-blue-600 dark:text-blue-400',
  PUT: 'bg-amber-500/15 text-amber-600 dark:text-amber-400',
  PATCH: 'bg-orange-500/15 text-orange-600 dark:text-orange-400',
  DELETE: 'bg-red-500/15 text-red-600 dark:text-red-400',
};

function CompactNodeComponent({ data, selected }: NodeProps<CompactNodeType>) {
  const { name, method, upstreamName, path_template, badgeColor, stepIndex, onSelect } = data;

  return (
    <div
      onClick={(e) => { e.stopPropagation(); onSelect(); }}
      className={`relative bg-card border rounded-lg shadow-sm w-[200px] text-foreground cursor-pointer transition-all ${
        selected ? 'border-primary ring-2 ring-primary/30' : 'border-border hover:border-primary/50'
      }`}
    >
      {/* Index badge */}
      <button
        type="button"
        style={{ backgroundColor: badgeColor }}
        className="absolute -top-2.5 -left-2.5 w-5 h-5 rounded-full text-white text-[10px] font-bold flex items-center justify-center shadow z-10"
      >
        {stepIndex}
      </button>

      <Handle type="target" position={Position.Left} className="!bg-primary !w-2.5 !h-2.5" />

      {/* Header — draggable */}
      <div className="compact-node-header px-3 py-2 cursor-grab">
        <div className="flex items-center gap-1.5">
          <span className={`px-1.5 py-0.5 text-[10px] font-bold rounded ${METHOD_COLORS[method] ?? 'bg-muted text-foreground'}`}>
            {method}
          </span>
          <span className="text-xs font-semibold truncate flex-1">
            {name || 'Unnamed'}
          </span>
        </div>
        <div className="mt-1 flex items-center gap-1 text-[10px] text-muted-foreground">
          <span className="truncate">{upstreamName}</span>
          <span className="mx-0.5">&middot;</span>
          <span className="font-mono truncate flex-1">{path_template || '/'}</span>
        </div>
      </div>

      <Handle type="source" position={Position.Right} className="!bg-primary !w-2.5 !h-2.5" />
    </div>
  );
}

export const CompactNode = memo(CompactNodeComponent);
