import { useState } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import {
  getApiKeys,
  getRoutes,
  createApiKey,
  updateApiKey,
  deleteApiKey,
  type ApiKey,
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
import { Plus, Trash2, Copy, AlertTriangle } from 'lucide-react';
import { useTimezone } from '../hooks/useTimezone';
import { formatDate } from '../lib/date';

export default function ApiKeysPage() {
  const qc = useQueryClient();
  const { timezone } = useTimezone();
  const apiKeys = useQuery({ queryKey: ['apiKeys'], queryFn: getApiKeys });
  const routes = useQuery({ queryKey: ['routes'], queryFn: getRoutes });
  const [modalOpen, setModalOpen] = useState(false);
  const [deleting, setDeleting] = useState<ApiKey | null>(null);
  const [createdKey, setCreatedKey] = useState<string | null>(null);

  // Form
  const [name, setName] = useState('');
  const [routeId, setRouteId] = useState('');
  const [expiresAt, setExpiresAt] = useState('');

  const createMut = useMutation({
    mutationFn: createApiKey,
    onSuccess: (data) => {
      qc.invalidateQueries({ queryKey: ['apiKeys'] });
      setModalOpen(false);
      if (data.plaintext_key) {
        setCreatedKey(data.plaintext_key);
      }
      toast.success('API key created');
    },
    onError: (e: any) => toast.error(e.response?.data?.error ?? 'Failed'),
  });

  const toggleMut = useMutation({
    mutationFn: ({ id, active }: { id: string; active: boolean }) => updateApiKey(id, { active }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['apiKeys'] });
      toast.success('API key updated');
    },
  });

  const deleteMut = useMutation({
    mutationFn: deleteApiKey,
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['apiKeys'] });
      setDeleting(null);
      toast.success('API key deleted');
    },
    onError: (e: any) => toast.error(e.response?.data?.error ?? 'Failed'),
  });

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    createMut.mutate({
      name,
      route_id: routeId || undefined,
      expires_at: expiresAt || undefined,
    });
  };

  const copyKey = () => {
    if (createdKey) {
      navigator.clipboard.writeText(createdKey);
      toast.success('Key copied to clipboard');
    }
  };

  const routeOptions = [
    { value: '', label: 'Global (all routes)' },
    ...(routes.data?.map((r) => ({ value: r.id, label: r.name })) ?? []),
  ];

  const getRouteName = (routeId: string | null) => {
    if (!routeId) return 'Global';
    return routes.data?.find((r) => r.id === routeId)?.name ?? routeId.slice(0, 8);
  };

  return (
    <div>
      <div className="flex items-center justify-between mb-6">
        <h1 className="text-2xl font-bold">API Keys</h1>
        <Button onClick={() => { setName(''); setRouteId(''); setExpiresAt(''); setModalOpen(true); }}>
          <Plus className="w-4 h-4 mr-1" /> Create API Key
        </Button>
      </div>

      <Card>
        {apiKeys.data?.length === 0 ? (
          <EmptyState
            message="No API keys created yet."
            action={<Button onClick={() => setModalOpen(true)}>Create your first API key</Button>}
          />
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b border-border text-left text-muted-foreground">
                  <th className="px-4 py-3 font-medium">Name</th>
                  <th className="px-4 py-3 font-medium">Scope</th>
                  <th className="px-4 py-3 font-medium">Status</th>
                  <th className="px-4 py-3 font-medium">Expires</th>
                  <th className="px-4 py-3 font-medium">Created</th>
                  <th className="px-4 py-3 font-medium">Actions</th>
                </tr>
              </thead>
              <tbody>
                {apiKeys.data?.map((key) => (
                  <tr key={key.id} className="border-b border-border last:border-0 hover:bg-muted/50">
                    <td className="px-4 py-3 font-medium">{key.name}</td>
                    <td className="px-4 py-3">
                      <Badge variant={key.route_id ? 'default' : 'muted'}>
                        {getRouteName(key.route_id)}
                      </Badge>
                    </td>
                    <td className="px-4 py-3">
                      <button
                        onClick={() => toggleMut.mutate({ id: key.id, active: !key.active })}
                        className={`text-xs px-2 py-0.5 rounded-full cursor-pointer ${
                          key.active
                            ? 'bg-success/10 text-success'
                            : 'bg-destructive/10 text-destructive'
                        }`}
                      >
                        {key.active ? 'Active' : 'Revoked'}
                      </button>
                    </td>
                    <td className="px-4 py-3 text-muted-foreground">
                      {key.expires_at
                        ? formatDate(key.expires_at, timezone)
                        : 'Never'}
                    </td>
                    <td className="px-4 py-3 text-muted-foreground">
                      {formatDate(key.created_at, timezone)}
                    </td>
                    <td className="px-4 py-3">
                      <button onClick={() => setDeleting(key)} className="p-1 hover:bg-muted rounded text-destructive cursor-pointer">
                        <Trash2 className="w-4 h-4" />
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </Card>

      {/* Create Modal */}
      <Dialog open={modalOpen} onOpenChange={setModalOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Create API Key</DialogTitle>
          </DialogHeader>
          <form onSubmit={handleSubmit} className="space-y-4">
            <div className="space-y-1">
              <Label>Name</Label>
              <Input value={name} onChange={(e) => setName(e.target.value)} placeholder="my-service-key" required />
            </div>
            <div className="space-y-1">
              <Label>Route Scope</Label>
              <Select value={routeId} onValueChange={setRouteId}>
                <SelectTrigger>
                  <SelectValue placeholder="Select scope" />
                </SelectTrigger>
                <SelectContent>
                  {routeOptions.map((opt) => (
                    <SelectItem key={opt.value} value={opt.value || '__global__'}>
                      {opt.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <div className="space-y-1">
              <Label>Expires At (optional)</Label>
              <Input type="datetime-local" value={expiresAt} onChange={(e) => setExpiresAt(e.target.value)} />
            </div>
            <DialogFooter>
              <Button variant="secondary" type="button" onClick={() => setModalOpen(false)}>Cancel</Button>
              <Button type="submit" disabled={createMut.isPending}>Create</Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>

      {/* Key Display Modal */}
      <Dialog open={!!createdKey} onOpenChange={(open) => !open && setCreatedKey(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>API Key Created</DialogTitle>
          </DialogHeader>
          <div className="space-y-4">
            <div className="flex items-start gap-2 p-3 bg-warning/10 rounded-md text-sm">
              <AlertTriangle className="w-4 h-4 text-warning mt-0.5 shrink-0" />
              <p>This key will not be shown again. Store it securely.</p>
            </div>
            <div className="flex gap-2">
              <Input
                readOnly
                value={createdKey ?? ''}
                className="flex-1 font-mono text-xs"
              />
              <Button variant="secondary" size="sm" onClick={copyKey}>
                <Copy className="w-4 h-4" />
              </Button>
            </div>
            <div className="flex justify-end">
              <Button onClick={() => setCreatedKey(null)}>I've saved this key</Button>
            </div>
          </div>
        </DialogContent>
      </Dialog>

      {/* Delete Confirmation */}
      <Dialog open={!!deleting} onOpenChange={(open) => !open && setDeleting(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Delete API Key</DialogTitle>
            <DialogDescription>
              Delete API key "{deleting?.name}"? Any clients using this key will lose access.
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
