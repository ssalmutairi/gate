import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import {
  getServices,
  importService,
  updateService,
  deleteService,
  type Service,
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
import { Plus, Trash2, Pencil, Search, Link, Upload, FileText } from 'lucide-react';
import { useTimezone } from '../hooks/useTimezone';
import { formatDate } from '../lib/date';

function slugify(input: string): string {
  return input
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '');
}

type ImportMethod = 'url' | 'file' | 'paste';

const STATUS_COLORS: Record<string, string> = {
  alpha: 'bg-purple-100 text-purple-800 dark:bg-purple-900 dark:text-purple-200',
  beta: 'bg-blue-100 text-blue-800 dark:bg-blue-900 dark:text-blue-200',
  stable: 'bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-200',
  deprecated: 'bg-red-100 text-red-800 dark:bg-red-900 dark:text-red-200',
};

export default function ServicesPage() {
  const navigate = useNavigate();
  const qc = useQueryClient();
  const { timezone } = useTimezone();

  // Filters
  const [search, setSearch] = useState('');
  const [statusFilter, setStatusFilter] = useState('');

  const services = useQuery({
    queryKey: ['services', search, statusFilter],
    queryFn: () => getServices({
      search: search || undefined,
      status: statusFilter || undefined,
    }),
  });

  const [modalOpen, setModalOpen] = useState(false);
  const [editService, setEditService] = useState<Service | null>(null);
  const [deleting, setDeleting] = useState<Service | null>(null);

  // Import form state
  const [importMethod, setImportMethod] = useState<ImportMethod>('url');
  const [url, setUrl] = useState('');
  const [specContent, setSpecContent] = useState('');
  const [namespace, setNamespace] = useState('');

  // Edit form state
  const [editDescription, setEditDescription] = useState('');
  const [editTags, setEditTags] = useState('');
  const [editStatus, setEditStatus] = useState('stable');

  const importMut = useMutation({
    mutationFn: importService,
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['services'] });
      qc.invalidateQueries({ queryKey: ['routes'] });
      qc.invalidateQueries({ queryKey: ['upstreams'] });
      setModalOpen(false);
      toast.success('Service imported successfully');
    },
    onError: (e: any) => {
      const msg = e?.response?.data?.error || e.message;
      toast.error(msg);
    },
  });

  const updateMut = useMutation({
    mutationFn: ({ id, data }: { id: string; data: { description?: string; tags?: string[]; status?: string } }) =>
      updateService(id, data),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['services'] });
      setEditService(null);
      toast.success('Service updated');
    },
    onError: (e: any) => toast.error(e?.response?.data?.error || e.message),
  });

  const deleteMut = useMutation({
    mutationFn: deleteService,
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['services'] });
      qc.invalidateQueries({ queryKey: ['routes'] });
      qc.invalidateQueries({ queryKey: ['upstreams'] });
      setDeleting(null);
      toast.success('Service deleted');
    },
    onError: (e: any) => toast.error(e?.response?.data?.error || e.message),
  });

  const handleImport = (e: React.FormEvent) => {
    e.preventDefault();
    const slug = slugify(namespace);
    if (!slug) return;
    if (importMethod === 'url') {
      importMut.mutate({ url, namespace: slug });
    } else {
      importMut.mutate({ spec_content: specContent, namespace: slug });
    }
  };

  const handleFileChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    const reader = new FileReader();
    reader.onload = () => setSpecContent(reader.result as string);
    reader.readAsText(file);
  };

  const openEdit = (svc: Service) => {
    setEditDescription(svc.description || '');
    setEditTags((svc.tags || []).join(', '));
    setEditStatus(svc.status || 'stable');
    setEditService(svc);
  };

  const handleUpdate = (e: React.FormEvent) => {
    e.preventDefault();
    if (!editService) return;
    const tags = editTags
      .split(',')
      .map((t) => t.trim())
      .filter(Boolean);
    updateMut.mutate({
      id: editService.id,
      data: { description: editDescription, tags, status: editStatus },
    });
  };

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-2xl font-bold">Services</h2>
          <p className="text-sm text-muted-foreground">
            Import OpenAPI/Swagger specs as namespaced services
          </p>
        </div>
        <Button
          onClick={() => {
            setImportMethod('url');
            setUrl('');
            setSpecContent('');
            setNamespace('');
            setModalOpen(true);
          }}
        >
          <Plus className="w-4 h-4 mr-2" />
          Import
        </Button>
      </div>

      {/* Search & Filter Bar */}
      <div className="flex gap-3">
        <div className="relative flex-1">
          <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-muted-foreground" />
          <Input
            type="text"
            placeholder="Search by namespace..."
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            className="pl-9"
          />
        </div>
        <Select value={statusFilter || '__all__'} onValueChange={(v) => setStatusFilter(v === '__all__' ? '' : v)}>
          <SelectTrigger className="w-[160px]">
            <SelectValue placeholder="All statuses" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="__all__">All statuses</SelectItem>
            <SelectItem value="alpha">Alpha</SelectItem>
            <SelectItem value="beta">Beta</SelectItem>
            <SelectItem value="stable">Stable</SelectItem>
            <SelectItem value="deprecated">Deprecated</SelectItem>
          </SelectContent>
        </Select>
      </div>

      <Card>
        {services.isLoading ? (
          <div className="p-8 text-center text-muted-foreground">Loading...</div>
        ) : !services.data?.length ? (
          <EmptyState
            message="No services found"
            action={
              <Button
                onClick={() => {
                  setImportMethod('url');
                  setUrl('');
                  setSpecContent('');
                  setNamespace('');
                  setModalOpen(true);
                }}
              >
                Import your first service
              </Button>
            }
          />
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b border-border text-left">
                  <th className="px-4 py-3 font-medium text-muted-foreground">Namespace</th>
                  <th className="px-4 py-3 font-medium text-muted-foreground">Version</th>
                  <th className="px-4 py-3 font-medium text-muted-foreground">Status</th>
                  <th className="px-4 py-3 font-medium text-muted-foreground">Description</th>
                  <th className="px-4 py-3 font-medium text-muted-foreground">Tags</th>
                  <th className="px-4 py-3 font-medium text-muted-foreground">Created</th>
                  <th className="px-4 py-3 font-medium text-muted-foreground w-24"></th>
                </tr>
              </thead>
              <tbody>
                {services.data.map((svc) => (
                  <tr key={svc.id} className="border-b border-border last:border-0 hover:bg-muted/50 cursor-pointer" onClick={() => navigate(`/services/${svc.id}`)}>
                    <td className="px-4 py-3 font-medium">
                      /{svc.namespace}
                    </td>
                    <td className="px-4 py-3">
                      <Badge>v{svc.version}</Badge>
                    </td>
                    <td className="px-4 py-3">
                      <span className={`inline-block px-2 py-0.5 rounded-full text-xs font-medium ${STATUS_COLORS[svc.status] || ''}`}>
                        {svc.status}
                      </span>
                    </td>
                    <td className="px-4 py-3 text-muted-foreground max-w-xs truncate">
                      {svc.description || '-'}
                    </td>
                    <td className="px-4 py-3">
                      <div className="flex gap-1 flex-wrap">
                        {(svc.tags || []).map((tag) => (
                          <Badge key={tag} variant="muted">{tag}</Badge>
                        ))}
                      </div>
                    </td>
                    <td className="px-4 py-3 text-muted-foreground">
                      {formatDate(svc.created_at, timezone)}
                    </td>
                    <td className="px-4 py-3">
                      <div className="flex gap-1">
                        <Button
                          variant="ghost"
                          size="sm"
                          onClick={(e) => { e.stopPropagation(); openEdit(svc); }}
                        >
                          <Pencil className="w-4 h-4" />
                        </Button>
                        <Button
                          variant="ghost"
                          size="sm"
                          onClick={(e) => { e.stopPropagation(); setDeleting(svc); }}
                        >
                          <Trash2 className="w-4 h-4 text-destructive" />
                        </Button>
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </Card>

      {/* Import Modal */}
      <Dialog open={modalOpen} onOpenChange={setModalOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Import OpenAPI Spec</DialogTitle>
          </DialogHeader>
          <form onSubmit={handleImport} className="space-y-4">
            {/* Method selector */}
            <div className="flex rounded-lg border border-border overflow-hidden">
              {([
                { key: 'url' as ImportMethod, label: 'URL', icon: Link },
                { key: 'file' as ImportMethod, label: 'File', icon: Upload },
                { key: 'paste' as ImportMethod, label: 'Paste', icon: FileText },
              ]).map(({ key, label, icon: Icon }) => (
                <button
                  key={key}
                  type="button"
                  onClick={() => setImportMethod(key)}
                  className={`flex-1 flex items-center justify-center gap-1.5 px-3 py-2 text-sm font-medium transition-colors cursor-pointer ${
                    importMethod === key
                      ? 'bg-primary text-primary-foreground'
                      : 'bg-background text-muted-foreground hover:bg-muted'
                  }`}
                >
                  <Icon className="w-4 h-4" />
                  {label}
                </button>
              ))}
            </div>

            {/* URL input */}
            {importMethod === 'url' && (
              <div className="space-y-1">
                <Label>Spec URL</Label>
                <Input
                  placeholder="https://petstore3.swagger.io/api/v3/openapi.json"
                  value={url}
                  onChange={(e) => setUrl(e.target.value)}
                  required
                />
              </div>
            )}

            {/* File upload */}
            {importMethod === 'file' && (
              <div>
                <Label className="mb-1 block">Spec File</Label>
                <input
                  type="file"
                  accept=".json"
                  onChange={handleFileChange}
                  className="w-full text-sm file:mr-3 file:px-3 file:py-1.5 file:rounded-md file:border-0 file:bg-primary file:text-primary-foreground file:text-sm file:font-medium file:cursor-pointer cursor-pointer"
                  required={!specContent}
                />
                {specContent && (
                  <p className="mt-1 text-xs text-green-600 dark:text-green-400">
                    File loaded ({Math.round(specContent.length / 1024)}KB)
                  </p>
                )}
              </div>
            )}

            {/* Paste JSON */}
            {importMethod === 'paste' && (
              <div>
                <Label className="mb-1 block">Spec JSON</Label>
                <textarea
                  value={specContent}
                  onChange={(e) => setSpecContent(e.target.value)}
                  placeholder='{"openapi":"3.0.0","info":{"title":"My API",...}}'
                  rows={8}
                  className="w-full px-3 py-2 rounded-md border border-input bg-transparent text-sm font-mono focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring resize-y"
                  required
                />
              </div>
            )}

            {/* Service Name + slug preview */}
            <div>
              <div className="space-y-1">
                <Label>Service Name</Label>
                <Input
                  placeholder="Pet Store"
                  value={namespace}
                  onChange={(e) => setNamespace(e.target.value)}
                  required
                />
              </div>
              {namespace && (
                <p className="mt-1 text-xs text-muted-foreground">
                  Path prefix: <code className="px-1 py-0.5 rounded bg-muted">/{slugify(namespace)}/...</code>
                </p>
              )}
            </div>

            <DialogFooter>
              <Button
                variant="secondary"
                type="button"
                onClick={() => setModalOpen(false)}
              >
                Cancel
              </Button>
              <Button type="submit" disabled={importMut.isPending}>
                {importMut.isPending ? 'Importing...' : 'Import'}
              </Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>

      {/* Edit Modal */}
      <Dialog open={!!editService} onOpenChange={(open) => !open && setEditService(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Edit Service: {editService?.namespace || ''}</DialogTitle>
          </DialogHeader>
          <form onSubmit={handleUpdate} className="space-y-4">
            <div className="space-y-1">
              <Label>Description</Label>
              <Input
                placeholder="A short description of this service"
                value={editDescription}
                onChange={(e) => setEditDescription(e.target.value)}
              />
            </div>
            <div className="space-y-1">
              <Label>Tags (comma-separated)</Label>
              <Input
                placeholder="rest, pets, public"
                value={editTags}
                onChange={(e) => setEditTags(e.target.value)}
              />
            </div>
            <div className="space-y-1">
              <Label>Status</Label>
              <Select value={editStatus} onValueChange={setEditStatus}>
                <SelectTrigger>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="alpha">Alpha</SelectItem>
                  <SelectItem value="beta">Beta</SelectItem>
                  <SelectItem value="stable">Stable</SelectItem>
                  <SelectItem value="deprecated">Deprecated</SelectItem>
                </SelectContent>
              </Select>
            </div>
            <DialogFooter>
              <Button
                variant="secondary"
                type="button"
                onClick={() => setEditService(null)}
              >
                Cancel
              </Button>
              <Button type="submit" disabled={updateMut.isPending}>
                {updateMut.isPending ? 'Saving...' : 'Save'}
              </Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>

      {/* Delete Confirmation */}
      <Dialog open={!!deleting} onOpenChange={(open) => !open && setDeleting(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Delete Service</DialogTitle>
            <DialogDescription>
              This will delete the "{deleting?.namespace}" service along with its upstream and route. This action cannot be undone.
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
