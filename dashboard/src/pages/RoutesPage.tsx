import { useState } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import {
  getRoutes,
  getUpstreams,
  createRoute,
  updateRoute,
  deleteRoute,
  type Route,
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

const ALL_METHODS = ['GET', 'POST', 'PUT', 'PATCH', 'DELETE'];

export default function RoutesPage() {
  const qc = useQueryClient();
  const routes = useQuery({ queryKey: ['routes'], queryFn: getRoutes });
  const upstreams = useQuery({ queryKey: ['upstreams'], queryFn: getUpstreams });
  const [modalOpen, setModalOpen] = useState(false);
  const [editing, setEditing] = useState<Route | null>(null);
  const [deleting, setDeleting] = useState<Route | null>(null);

  // Form state
  const [name, setName] = useState('');
  const [pathPrefix, setPathPrefix] = useState('');
  const [methods, setMethods] = useState<string[]>([]);
  const [upstreamId, setUpstreamId] = useState('');
  const [stripPrefix, setStripPrefix] = useState(false);

  const openCreate = () => {
    setEditing(null);
    setName('');
    setPathPrefix('');
    setMethods([]);
    setUpstreamId(upstreams.data?.[0]?.id ?? '');
    setStripPrefix(false);
    setModalOpen(true);
  };

  const openEdit = (route: Route) => {
    setEditing(route);
    setName(route.name);
    setPathPrefix(route.path_prefix);
    setMethods(route.methods ?? []);
    setUpstreamId(route.upstream_id);
    setStripPrefix(route.strip_prefix);
    setModalOpen(true);
  };

  const createMut = useMutation({
    mutationFn: (data: Parameters<typeof createRoute>[0]) => createRoute(data),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['routes'] });
      setModalOpen(false);
      toast('success', 'Route created');
    },
    onError: (e: any) => toast('error', e.response?.data?.error ?? 'Failed to create route'),
  });

  const updateMut = useMutation({
    mutationFn: ({ id, data }: { id: string; data: Parameters<typeof updateRoute>[1] }) =>
      updateRoute(id, data),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['routes'] });
      setModalOpen(false);
      toast('success', 'Route updated');
    },
    onError: (e: any) => toast('error', e.response?.data?.error ?? 'Failed to update route'),
  });

  const deleteMut = useMutation({
    mutationFn: deleteRoute,
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['routes'] });
      setDeleting(null);
      toast('success', 'Route deleted');
    },
    onError: (e: any) => toast('error', e.response?.data?.error ?? 'Failed to delete route'),
  });

  const toggleActive = useMutation({
    mutationFn: ({ id, active }: { id: string; active: boolean }) =>
      updateRoute(id, { active }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['routes'] });
      toast('success', 'Route status updated');
    },
  });

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    const data = {
      name,
      path_prefix: pathPrefix,
      methods: methods.length > 0 ? methods : undefined,
      upstream_id: upstreamId,
      strip_prefix: stripPrefix,
    };
    if (editing) {
      updateMut.mutate({ id: editing.id, data });
    } else {
      createMut.mutate(data);
    }
  };

  const toggleMethod = (method: string) => {
    setMethods((prev) =>
      prev.includes(method) ? prev.filter((m) => m !== method) : [...prev, method]
    );
  };

  return (
    <div>
      <div className="flex items-center justify-between mb-6">
        <h1 className="text-2xl font-bold">Routes</h1>
        <Button onClick={openCreate}>
          <Plus className="w-4 h-4 mr-1" /> Create Route
        </Button>
      </div>

      <Card>
        {routes.data?.length === 0 ? (
          <EmptyState
            message="No routes configured yet."
            action={<Button onClick={openCreate}>Create your first route</Button>}
          />
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b border-border text-left text-muted-foreground">
                  <th className="px-4 py-3 font-medium">Name</th>
                  <th className="px-4 py-3 font-medium">Path Prefix</th>
                  <th className="px-4 py-3 font-medium">Methods</th>
                  <th className="px-4 py-3 font-medium">Upstream</th>
                  <th className="px-4 py-3 font-medium">Status</th>
                  <th className="px-4 py-3 font-medium">Actions</th>
                </tr>
              </thead>
              <tbody>
                {routes.data?.map((route) => (
                  <tr key={route.id} className="border-b border-border last:border-0 hover:bg-muted/50">
                    <td className="px-4 py-3 font-medium">{route.name}</td>
                    <td className="px-4 py-3 font-mono text-xs">{route.path_prefix}</td>
                    <td className="px-4 py-3">
                      {route.methods?.length ? (
                        <div className="flex gap-1 flex-wrap">
                          {route.methods.map((m) => (
                            <Badge key={m} variant="muted">{m}</Badge>
                          ))}
                        </div>
                      ) : (
                        <span className="text-muted-foreground">All</span>
                      )}
                    </td>
                    <td className="px-4 py-3">{route.upstream_name ?? route.upstream_id.slice(0, 8)}</td>
                    <td className="px-4 py-3">
                      <button
                        onClick={() => toggleActive.mutate({ id: route.id, active: !route.active })}
                        className={`text-xs px-2 py-0.5 rounded-full cursor-pointer ${
                          route.active
                            ? 'bg-success/10 text-success'
                            : 'bg-muted text-muted-foreground'
                        }`}
                      >
                        {route.active ? 'Active' : 'Inactive'}
                      </button>
                    </td>
                    <td className="px-4 py-3">
                      <div className="flex gap-1">
                        <button onClick={() => openEdit(route)} className="p-1 hover:bg-muted rounded cursor-pointer">
                          <Pencil className="w-4 h-4" />
                        </button>
                        <button onClick={() => setDeleting(route)} className="p-1 hover:bg-muted rounded text-destructive cursor-pointer">
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
        title={editing ? 'Edit Route' : 'Create Route'}
      >
        <form onSubmit={handleSubmit} className="space-y-4">
          <Input label="Name" value={name} onChange={(e) => setName(e.target.value)} required />
          <Input
            label="Path Prefix"
            value={pathPrefix}
            onChange={(e) => setPathPrefix(e.target.value)}
            placeholder="/api/v1"
            required
          />
          <div className="space-y-1">
            <label className="text-sm font-medium">Methods (empty = all)</label>
            <div className="flex gap-2 flex-wrap">
              {ALL_METHODS.map((m) => (
                <button
                  key={m}
                  type="button"
                  onClick={() => toggleMethod(m)}
                  className={`px-2 py-1 text-xs rounded border cursor-pointer ${
                    methods.includes(m)
                      ? 'bg-primary text-primary-foreground border-primary'
                      : 'border-border hover:bg-muted'
                  }`}
                >
                  {m}
                </button>
              ))}
            </div>
          </div>
          <Select
            label="Upstream"
            value={upstreamId}
            onChange={(e) => setUpstreamId(e.target.value)}
            options={
              upstreams.data?.map((u) => ({ value: u.id, label: u.name })) ?? []
            }
          />
          <label className="flex items-center gap-2 text-sm">
            <input
              type="checkbox"
              checked={stripPrefix}
              onChange={(e) => setStripPrefix(e.target.checked)}
            />
            Strip path prefix
          </label>
          <div className="flex justify-end gap-2 pt-2">
            <Button variant="secondary" type="button" onClick={() => setModalOpen(false)}>
              Cancel
            </Button>
            <Button type="submit" disabled={createMut.isPending || updateMut.isPending}>
              {editing ? 'Update' : 'Create'}
            </Button>
          </div>
        </form>
      </Modal>

      {/* Delete Confirmation */}
      <ConfirmDialog
        open={!!deleting}
        onClose={() => setDeleting(null)}
        onConfirm={() => deleting && deleteMut.mutate(deleting.id)}
        title="Delete Route"
        message={`Are you sure you want to delete "${deleting?.name}"? This action cannot be undone.`}
      />
    </div>
  );
}
