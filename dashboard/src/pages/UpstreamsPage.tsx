import { useState } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import {
  getUpstreams,
  createUpstream,
  updateUpstream,
  deleteUpstream,
  createTarget,
  deleteTarget,
  type Upstream,
  type Target,
} from '../lib/api';
import {
  Button,
  Card,
  Modal,
  Input,
  Select,
  Badge,
  ConfirmDialog,
  EmptyState,
  toast,
} from '../components/ui';
import { Plus, Pencil, Trash2, ChevronDown, ChevronRight, Circle } from 'lucide-react';

export default function UpstreamsPage() {
  const qc = useQueryClient();
  const upstreams = useQuery({ queryKey: ['upstreams'], queryFn: getUpstreams });
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [modalOpen, setModalOpen] = useState(false);
  const [editing, setEditing] = useState<Upstream | null>(null);
  const [deleting, setDeleting] = useState<Upstream | null>(null);
  const [deletingTarget, setDeletingTarget] = useState<{ upstream: Upstream; target: Target } | null>(null);
  const [targetModal, setTargetModal] = useState<string | null>(null);

  // Upstream form
  const [name, setName] = useState('');
  const [algorithm, setAlgorithm] = useState('round_robin');

  // Target form
  const [targetHost, setTargetHost] = useState('');
  const [targetPort, setTargetPort] = useState('80');
  const [targetWeight, setTargetWeight] = useState('1');

  const toggle = (id: string) => {
    setExpanded((prev) => {
      const next = new Set(prev);
      next.has(id) ? next.delete(id) : next.add(id);
      return next;
    });
  };

  const openCreate = () => {
    setEditing(null);
    setName('');
    setAlgorithm('round_robin');
    setModalOpen(true);
  };

  const openEdit = (upstream: Upstream) => {
    setEditing(upstream);
    setName(upstream.name);
    setAlgorithm(upstream.algorithm);
    setModalOpen(true);
  };

  const createMut = useMutation({
    mutationFn: createUpstream,
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['upstreams'] });
      setModalOpen(false);
      toast('success', 'Upstream created');
    },
    onError: (e: any) => toast('error', e.response?.data?.error ?? 'Failed'),
  });

  const updateMut = useMutation({
    mutationFn: ({ id, data }: { id: string; data: Parameters<typeof updateUpstream>[1] }) =>
      updateUpstream(id, data),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['upstreams'] });
      setModalOpen(false);
      toast('success', 'Upstream updated');
    },
    onError: (e: any) => toast('error', e.response?.data?.error ?? 'Failed'),
  });

  const deleteMut = useMutation({
    mutationFn: deleteUpstream,
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['upstreams'] });
      setDeleting(null);
      toast('success', 'Upstream deleted');
    },
    onError: (e: any) => toast('error', e.response?.data?.error ?? 'Failed'),
  });

  const createTargetMut = useMutation({
    mutationFn: ({ upstreamId, data }: { upstreamId: string; data: Parameters<typeof createTarget>[1] }) =>
      createTarget(upstreamId, data),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['upstreams'] });
      setTargetModal(null);
      toast('success', 'Target added');
    },
    onError: (e: any) => toast('error', e.response?.data?.error ?? 'Failed'),
  });

  const deleteTargetMut = useMutation({
    mutationFn: ({ upstreamId, targetId }: { upstreamId: string; targetId: string }) =>
      deleteTarget(upstreamId, targetId),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['upstreams'] });
      setDeletingTarget(null);
      toast('success', 'Target removed');
    },
    onError: (e: any) => toast('error', e.response?.data?.error ?? 'Failed'),
  });

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (editing) {
      updateMut.mutate({ id: editing.id, data: { name, algorithm } });
    } else {
      createMut.mutate({ name, algorithm });
    }
  };

  const handleTargetSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!targetModal) return;
    createTargetMut.mutate({
      upstreamId: targetModal,
      data: { host: targetHost, port: parseInt(targetPort), weight: parseInt(targetWeight) },
    });
  };

  const algOptions = [
    { value: 'round_robin', label: 'Round Robin' },
    { value: 'weighted_round_robin', label: 'Weighted Round Robin' },
    { value: 'least_connections', label: 'Least Connections' },
  ];

  return (
    <div>
      <div className="flex items-center justify-between mb-6">
        <h1 className="text-2xl font-bold">Upstreams</h1>
        <Button onClick={openCreate}>
          <Plus className="w-4 h-4 mr-1" /> Create Upstream
        </Button>
      </div>

      {upstreams.data?.length === 0 ? (
        <Card>
          <EmptyState
            message="No upstreams configured yet."
            action={<Button onClick={openCreate}>Create your first upstream</Button>}
          />
        </Card>
      ) : (
        <div className="space-y-3">
          {upstreams.data?.map((upstream) => (
            <Card key={upstream.id}>
              <div
                className="flex items-center justify-between px-4 py-3 cursor-pointer hover:bg-muted/50"
                onClick={() => toggle(upstream.id)}
              >
                <div className="flex items-center gap-2">
                  {expanded.has(upstream.id) ? (
                    <ChevronDown className="w-4 h-4" />
                  ) : (
                    <ChevronRight className="w-4 h-4" />
                  )}
                  <span className="font-medium">{upstream.name}</span>
                  <Badge variant="muted">{upstream.algorithm.replace(/_/g, ' ')}</Badge>
                  <span className="text-xs text-muted-foreground">
                    {upstream.targets?.length ?? 0} target(s)
                  </span>
                </div>
                <div className="flex gap-1" onClick={(e) => e.stopPropagation()}>
                  <button onClick={() => openEdit(upstream)} className="p-1 hover:bg-muted rounded cursor-pointer">
                    <Pencil className="w-4 h-4" />
                  </button>
                  <button onClick={() => setDeleting(upstream)} className="p-1 hover:bg-muted rounded text-destructive cursor-pointer">
                    <Trash2 className="w-4 h-4" />
                  </button>
                </div>
              </div>

              {expanded.has(upstream.id) && (
                <div className="px-4 pb-3 border-t border-border">
                  <div className="flex items-center justify-between py-2">
                    <span className="text-sm font-medium text-muted-foreground">Targets</span>
                    <Button
                      size="sm"
                      variant="secondary"
                      onClick={() => {
                        setTargetHost('');
                        setTargetPort('80');
                        setTargetWeight('1');
                        setTargetModal(upstream.id);
                      }}
                    >
                      <Plus className="w-3 h-3 mr-1" /> Add Target
                    </Button>
                  </div>
                  {upstream.targets?.length === 0 ? (
                    <p className="text-sm text-muted-foreground py-2">No targets</p>
                  ) : (
                    <div className="space-y-1">
                      {upstream.targets?.map((target) => (
                        <div
                          key={target.id}
                          className="flex items-center justify-between py-1.5 text-sm"
                        >
                          <div className="flex items-center gap-2">
                            <Circle
                              className={`w-2.5 h-2.5 fill-current ${
                                target.healthy ? 'text-success' : 'text-destructive'
                              }`}
                            />
                            <span className="font-mono">
                              {target.host}:{target.port}
                            </span>
                            <span className="text-muted-foreground">w={target.weight}</span>
                          </div>
                          <button
                            onClick={() => setDeletingTarget({ upstream, target })}
                            className="p-1 hover:bg-muted rounded text-destructive cursor-pointer"
                          >
                            <Trash2 className="w-3 h-3" />
                          </button>
                        </div>
                      ))}
                    </div>
                  )}
                </div>
              )}
            </Card>
          ))}
        </div>
      )}

      {/* Create/Edit Upstream Modal */}
      <Modal
        open={modalOpen}
        onClose={() => setModalOpen(false)}
        title={editing ? 'Edit Upstream' : 'Create Upstream'}
      >
        <form onSubmit={handleSubmit} className="space-y-4">
          <Input label="Name" value={name} onChange={(e) => setName(e.target.value)} required />
          <Select label="Algorithm" value={algorithm} onChange={(e) => setAlgorithm(e.target.value)} options={algOptions} />
          <div className="flex justify-end gap-2 pt-2">
            <Button variant="secondary" type="button" onClick={() => setModalOpen(false)}>Cancel</Button>
            <Button type="submit">{editing ? 'Update' : 'Create'}</Button>
          </div>
        </form>
      </Modal>

      {/* Add Target Modal */}
      <Modal
        open={!!targetModal}
        onClose={() => setTargetModal(null)}
        title="Add Target"
      >
        <form onSubmit={handleTargetSubmit} className="space-y-4">
          <Input label="Host" value={targetHost} onChange={(e) => setTargetHost(e.target.value)} placeholder="api.example.com" required />
          <Input label="Port" type="number" value={targetPort} onChange={(e) => setTargetPort(e.target.value)} required />
          <Input label="Weight" type="number" value={targetWeight} onChange={(e) => setTargetWeight(e.target.value)} min={1} />
          <div className="flex justify-end gap-2 pt-2">
            <Button variant="secondary" type="button" onClick={() => setTargetModal(null)}>Cancel</Button>
            <Button type="submit">Add Target</Button>
          </div>
        </form>
      </Modal>

      {/* Delete Upstream Confirmation */}
      <ConfirmDialog
        open={!!deleting}
        onClose={() => setDeleting(null)}
        onConfirm={() => deleting && deleteMut.mutate(deleting.id)}
        title="Delete Upstream"
        message={`Delete "${deleting?.name}" and all its targets?`}
      />

      {/* Delete Target Confirmation */}
      <ConfirmDialog
        open={!!deletingTarget}
        onClose={() => setDeletingTarget(null)}
        onConfirm={() =>
          deletingTarget &&
          deleteTargetMut.mutate({
            upstreamId: deletingTarget.upstream.id,
            targetId: deletingTarget.target.id,
          })
        }
        title="Remove Target"
        message={`Remove target ${deletingTarget?.target.host}:${deletingTarget?.target.port}?`}
        confirmLabel="Remove"
      />
    </div>
  );
}
