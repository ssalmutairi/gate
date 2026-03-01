import { useState } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import {
  getRateLimits,
  getRoutes,
  createRateLimit,
  updateRateLimit,
  deleteRateLimit,
  type RateLimit,
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
import { Plus, Pencil, Trash2 } from 'lucide-react';

export default function RateLimitsPage() {
  const qc = useQueryClient();
  const rateLimits = useQuery({ queryKey: ['rateLimits'], queryFn: getRateLimits });
  const routes = useQuery({ queryKey: ['routes'], queryFn: getRoutes });
  const [modalOpen, setModalOpen] = useState(false);
  const [editing, setEditing] = useState<RateLimit | null>(null);
  const [deleting, setDeleting] = useState<RateLimit | null>(null);

  // Form
  const [routeId, setRouteId] = useState('');
  const [rps, setRps] = useState('10');
  const [rpm, setRpm] = useState('');
  const [rph, setRph] = useState('');
  const [limitBy, setLimitBy] = useState('ip');

  const openCreate = () => {
    setEditing(null);
    setRouteId(routes.data?.[0]?.id ?? '');
    setRps('10');
    setRpm('');
    setRph('');
    setLimitBy('ip');
    setModalOpen(true);
  };

  const openEdit = (rl: RateLimit) => {
    setEditing(rl);
    setRouteId(rl.route_id);
    setRps(String(rl.requests_per_second));
    setRpm(rl.requests_per_minute ? String(rl.requests_per_minute) : '');
    setRph(rl.requests_per_hour ? String(rl.requests_per_hour) : '');
    setLimitBy(rl.limit_by);
    setModalOpen(true);
  };

  const createMut = useMutation({
    mutationFn: createRateLimit,
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['rateLimits'] });
      setModalOpen(false);
      toast('success', 'Rate limit created');
    },
    onError: (e: any) => toast('error', e.response?.data?.error ?? 'Failed'),
  });

  const updateMut = useMutation({
    mutationFn: ({ id, data }: { id: string; data: Parameters<typeof updateRateLimit>[1] }) =>
      updateRateLimit(id, data),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['rateLimits'] });
      setModalOpen(false);
      toast('success', 'Rate limit updated');
    },
    onError: (e: any) => toast('error', e.response?.data?.error ?? 'Failed'),
  });

  const deleteMut = useMutation({
    mutationFn: deleteRateLimit,
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['rateLimits'] });
      setDeleting(null);
      toast('success', 'Rate limit deleted');
    },
    onError: (e: any) => toast('error', e.response?.data?.error ?? 'Failed'),
  });

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    const data = {
      route_id: routeId,
      requests_per_second: parseInt(rps),
      requests_per_minute: rpm ? parseInt(rpm) : undefined,
      requests_per_hour: rph ? parseInt(rph) : undefined,
      limit_by: limitBy,
    };
    if (editing) {
      updateMut.mutate({ id: editing.id, data });
    } else {
      createMut.mutate(data);
    }
  };

  const getRouteName = (routeId: string) =>
    routes.data?.find((r) => r.id === routeId)?.name ?? routeId.slice(0, 8);

  return (
    <div>
      <div className="flex items-center justify-between mb-6">
        <h1 className="text-2xl font-bold">Rate Limits</h1>
        <Button onClick={openCreate}>
          <Plus className="w-4 h-4 mr-1" /> Create Rate Limit
        </Button>
      </div>

      <Card>
        {rateLimits.data?.length === 0 ? (
          <EmptyState
            message="No rate limits configured yet."
            action={<Button onClick={openCreate}>Create your first rate limit</Button>}
          />
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b border-border text-left text-muted-foreground">
                  <th className="px-4 py-3 font-medium">Route</th>
                  <th className="px-4 py-3 font-medium">Req/sec</th>
                  <th className="px-4 py-3 font-medium">Req/min</th>
                  <th className="px-4 py-3 font-medium">Req/hour</th>
                  <th className="px-4 py-3 font-medium">Limit By</th>
                  <th className="px-4 py-3 font-medium">Actions</th>
                </tr>
              </thead>
              <tbody>
                {rateLimits.data?.map((rl) => (
                  <tr key={rl.id} className="border-b border-border last:border-0 hover:bg-muted/50">
                    <td className="px-4 py-3 font-medium">
                      {rl.route_name ?? getRouteName(rl.route_id)}
                    </td>
                    <td className="px-4 py-3">{rl.requests_per_second}</td>
                    <td className="px-4 py-3 text-muted-foreground">
                      {rl.requests_per_minute ?? '-'}
                    </td>
                    <td className="px-4 py-3 text-muted-foreground">
                      {rl.requests_per_hour ?? '-'}
                    </td>
                    <td className="px-4 py-3">
                      <Badge variant="muted">{rl.limit_by}</Badge>
                    </td>
                    <td className="px-4 py-3">
                      <div className="flex gap-1">
                        <button onClick={() => openEdit(rl)} className="p-1 hover:bg-muted rounded cursor-pointer">
                          <Pencil className="w-4 h-4" />
                        </button>
                        <button onClick={() => setDeleting(rl)} className="p-1 hover:bg-muted rounded text-destructive cursor-pointer">
                          <Trash2 className="w-4 h-4" />
                        </button>
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </Card>

      {/* Create/Edit Modal */}
      <Modal
        open={modalOpen}
        onClose={() => setModalOpen(false)}
        title={editing ? 'Edit Rate Limit' : 'Create Rate Limit'}
      >
        <form onSubmit={handleSubmit} className="space-y-4">
          <Select
            label="Route"
            value={routeId}
            onChange={(e) => setRouteId(e.target.value)}
            options={routes.data?.map((r) => ({ value: r.id, label: r.name })) ?? []}
          />
          <Input label="Requests per second" type="number" value={rps} onChange={(e) => setRps(e.target.value)} min={1} required />
          <Input label="Requests per minute (optional)" type="number" value={rpm} onChange={(e) => setRpm(e.target.value)} min={1} />
          <Input label="Requests per hour (optional)" type="number" value={rph} onChange={(e) => setRph(e.target.value)} min={1} />
          <Select
            label="Limit By"
            value={limitBy}
            onChange={(e) => setLimitBy(e.target.value)}
            options={[
              { value: 'ip', label: 'IP Address' },
              { value: 'api_key', label: 'API Key' },
            ]}
          />
          <div className="flex justify-end gap-2 pt-2">
            <Button variant="secondary" type="button" onClick={() => setModalOpen(false)}>Cancel</Button>
            <Button type="submit">{editing ? 'Update' : 'Create'}</Button>
          </div>
        </form>
      </Modal>

      {/* Delete Confirmation */}
      <ConfirmDialog
        open={!!deleting}
        onClose={() => setDeleting(null)}
        onConfirm={() => deleting && deleteMut.mutate(deleting.id)}
        title="Delete Rate Limit"
        message={`Delete rate limit for route "${deleting ? getRouteName(deleting.route_id) : ''}"?`}
      />
    </div>
  );
}
