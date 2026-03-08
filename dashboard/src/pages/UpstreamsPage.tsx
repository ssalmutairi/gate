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
import { Button } from '../components/ui/button';
import { Card } from '../components/ui/card';
import { Badge } from '../components/ui/badge';
import { Input } from '../components/ui/input';
import { Label } from '../components/ui/label';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from '../components/ui/dialog';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '../components/ui/select';
import { EmptyState } from '../components/ui';
import { toast } from 'sonner';
import { Plus, Pencil, Trash2, ChevronDown, ChevronRight, Circle, Lock } from 'lucide-react';

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
  const [tlsOpen, setTlsOpen] = useState(false);
  const [tlsCaCert, setTlsCaCert] = useState('');
  const [tlsClientCert, setTlsClientCert] = useState('');
  const [tlsClientKey, setTlsClientKey] = useState('');
  const [tlsSkipVerify, setTlsSkipVerify] = useState(false);

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
    setTlsCaCert('');
    setTlsClientCert('');
    setTlsClientKey('');
    setTlsSkipVerify(false);
    setTlsOpen(false);
    setModalOpen(true);
  };

  const openEdit = (upstream: Upstream) => {
    setEditing(upstream);
    setName(upstream.name);
    setAlgorithm(upstream.algorithm);
    setTlsCaCert(upstream.tls_ca_cert ?? '');
    setTlsClientCert(upstream.tls_client_cert ?? '');
    setTlsClientKey(upstream.tls_client_key ?? '');
    setTlsSkipVerify(upstream.tls_skip_verify);
    setTlsOpen(!!(upstream.tls_ca_cert || upstream.tls_client_cert || upstream.tls_skip_verify));
    setModalOpen(true);
  };

  const createMut = useMutation({
    mutationFn: createUpstream,
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['upstreams'] });
      setModalOpen(false);
      toast.success('Upstream created');
    },
    onError: (e: any) => toast.error(e.response?.data?.error ?? 'Failed'),
  });

  const updateMut = useMutation({
    mutationFn: ({ id, data }: { id: string; data: Parameters<typeof updateUpstream>[1] }) =>
      updateUpstream(id, data),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['upstreams'] });
      setModalOpen(false);
      toast.success('Upstream updated');
    },
    onError: (e: any) => toast.error(e.response?.data?.error ?? 'Failed'),
  });

  const deleteMut = useMutation({
    mutationFn: deleteUpstream,
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['upstreams'] });
      setDeleting(null);
      toast.success('Upstream deleted');
    },
    onError: (e: any) => toast.error(e.response?.data?.error ?? 'Failed'),
  });

  const createTargetMut = useMutation({
    mutationFn: ({ upstreamId, data }: { upstreamId: string; data: Parameters<typeof createTarget>[1] }) =>
      createTarget(upstreamId, data),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['upstreams'] });
      setTargetModal(null);
      toast.success('Target added');
    },
    onError: (e: any) => toast.error(e.response?.data?.error ?? 'Failed'),
  });

  const deleteTargetMut = useMutation({
    mutationFn: ({ upstreamId, targetId }: { upstreamId: string; targetId: string }) =>
      deleteTarget(upstreamId, targetId),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['upstreams'] });
      setDeletingTarget(null);
      toast.success('Target removed');
    },
    onError: (e: any) => toast.error(e.response?.data?.error ?? 'Failed'),
  });

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    const tlsData = {
      tls_ca_cert: tlsCaCert || null,
      tls_client_cert: tlsClientCert || null,
      tls_client_key: tlsClientKey || null,
      tls_skip_verify: tlsSkipVerify,
    };
    if (editing) {
      updateMut.mutate({ id: editing.id, data: { name, algorithm, ...tlsData } });
    } else {
      createMut.mutate({ name, algorithm, ...tlsData });
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
                  {(upstream.tls_ca_cert || upstream.tls_client_cert || upstream.tls_skip_verify) && (
                    <Badge variant="muted" className="gap-1">
                      <Lock className="w-3 h-3" /> TLS
                    </Badge>
                  )}
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
      <Dialog open={modalOpen} onOpenChange={setModalOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{editing ? 'Edit Upstream' : 'Create Upstream'}</DialogTitle>
          </DialogHeader>
          <form onSubmit={handleSubmit} className="space-y-4">
            <div className="space-y-1">
              <Label>Name</Label>
              <Input value={name} onChange={(e) => setName(e.target.value)} required />
            </div>
            <div className="space-y-1">
              <Label>Algorithm</Label>
              <Select value={algorithm} onValueChange={setAlgorithm}>
                <SelectTrigger>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="round_robin">Round Robin</SelectItem>
                  <SelectItem value="weighted_round_robin">Weighted Round Robin</SelectItem>
                  <SelectItem value="least_connections">Least Connections</SelectItem>
                </SelectContent>
              </Select>
            </div>
            <div className="border rounded-md">
              <button
                type="button"
                className="flex items-center justify-between w-full px-3 py-2 text-sm font-medium cursor-pointer"
                onClick={() => setTlsOpen(!tlsOpen)}
              >
                <span className="flex items-center gap-1.5">
                  <Lock className="w-3.5 h-3.5" /> TLS Configuration
                </span>
                {tlsOpen ? <ChevronDown className="w-4 h-4" /> : <ChevronRight className="w-4 h-4" />}
              </button>
              {tlsOpen && (
                <div className="px-3 pb-3 space-y-3 border-t">
                  <div className="space-y-1 pt-2">
                    <Label>CA Certificate</Label>
                    <textarea
                      className="flex min-h-[80px] w-full rounded-md border border-input bg-background px-3 py-2 text-sm font-mono"
                      value={tlsCaCert}
                      onChange={(e) => setTlsCaCert(e.target.value)}
                      placeholder="Paste PEM-encoded CA certificate..."
                    />
                  </div>
                  <div className="space-y-1">
                    <Label>Client Certificate</Label>
                    <textarea
                      className="flex min-h-[80px] w-full rounded-md border border-input bg-background px-3 py-2 text-sm font-mono"
                      value={tlsClientCert}
                      onChange={(e) => setTlsClientCert(e.target.value)}
                      placeholder="Paste PEM-encoded client certificate..."
                    />
                  </div>
                  <div className="space-y-1">
                    <Label>Client Key</Label>
                    <textarea
                      className="flex min-h-[80px] w-full rounded-md border border-input bg-background px-3 py-2 text-sm font-mono"
                      value={tlsClientKey}
                      onChange={(e) => setTlsClientKey(e.target.value)}
                      placeholder="Paste PEM-encoded private key..."
                    />
                  </div>
                  <div className="flex items-center gap-2">
                    <input
                      type="checkbox"
                      id="tls-skip-verify"
                      checked={tlsSkipVerify}
                      onChange={(e) => setTlsSkipVerify(e.target.checked)}
                      className="h-4 w-4 rounded border-input"
                    />
                    <Label htmlFor="tls-skip-verify" className="text-sm font-normal">
                      Skip TLS verification
                    </Label>
                  </div>
                </div>
              )}
            </div>
            <DialogFooter>
              <Button variant="secondary" type="button" onClick={() => setModalOpen(false)}>Cancel</Button>
              <Button type="submit">{editing ? 'Update' : 'Create'}</Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>

      {/* Add Target Modal */}
      <Dialog open={!!targetModal} onOpenChange={(open) => !open && setTargetModal(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Add Target</DialogTitle>
          </DialogHeader>
          <form onSubmit={handleTargetSubmit} className="space-y-4">
            <div className="space-y-1">
              <Label>Host</Label>
              <Input value={targetHost} onChange={(e) => setTargetHost(e.target.value)} placeholder="api.example.com" required />
            </div>
            <div className="space-y-1">
              <Label>Port</Label>
              <Input type="number" value={targetPort} onChange={(e) => setTargetPort(e.target.value)} required />
            </div>
            <div className="space-y-1">
              <Label>Weight</Label>
              <Input type="number" value={targetWeight} onChange={(e) => setTargetWeight(e.target.value)} min={1} />
            </div>
            <DialogFooter>
              <Button variant="secondary" type="button" onClick={() => setTargetModal(null)}>Cancel</Button>
              <Button type="submit">Add Target</Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>

      {/* Delete Upstream Confirmation */}
      <Dialog open={!!deleting} onOpenChange={(open) => !open && setDeleting(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Delete Upstream</DialogTitle>
            <DialogDescription>
              Delete "{deleting?.name}" and all its targets?
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="secondary" onClick={() => setDeleting(null)}>Cancel</Button>
            <Button variant="destructive" onClick={() => deleting && deleteMut.mutate(deleting.id)}>Delete</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Delete Target Confirmation */}
      <Dialog open={!!deletingTarget} onOpenChange={(open) => !open && setDeletingTarget(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Remove Target</DialogTitle>
            <DialogDescription>
              Remove target {deletingTarget?.target.host}:{deletingTarget?.target.port}?
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="secondary" onClick={() => setDeletingTarget(null)}>Cancel</Button>
            <Button
              variant="destructive"
              onClick={() =>
                deletingTarget &&
                deleteTargetMut.mutate({
                  upstreamId: deletingTarget.upstream.id,
                  targetId: deletingTarget.target.id,
                })
              }
            >
              Remove
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
