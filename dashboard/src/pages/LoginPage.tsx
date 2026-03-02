import { useState } from 'react';
import { Card } from '../components/ui/card';
import { Input } from '../components/ui/input';
import { Button } from '../components/ui/button';
import { Label } from '../components/ui/label';
import { toast } from 'sonner';
import { Loader2 } from 'lucide-react';
import axios from 'axios';

export default function LoginPage({ onLogin }: { onLogin: (token: string) => void }) {
  const [token, setToken] = useState('');
  const [loading, setLoading] = useState(false);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    const trimmed = token.trim();
    if (!trimmed) return;

    setLoading(true);
    try {
      await axios.get('/admin/health', {
        headers: { 'X-Admin-Token': trimmed },
      });
      onLogin(trimmed);
    } catch {
      toast.error('Invalid admin token');
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="min-h-screen flex items-center justify-center bg-muted/30 p-4">
      <Card className="w-full max-w-sm p-8">
        <div className="flex flex-col items-center gap-2 mb-6">
          <img src="/gate-logo.png" alt="Gate" className="h-10 w-auto" />
          <h1 className="text-xl font-bold">Gate</h1>
          <p className="text-sm text-muted-foreground">Enter your admin token to continue</p>
        </div>
        <form onSubmit={handleSubmit} className="space-y-4">
          <div className="space-y-2">
            <Label htmlFor="token">Admin Token</Label>
            <Input
              id="token"
              type="password"
              placeholder="Enter token..."
              value={token}
              onChange={(e) => setToken(e.target.value)}
              autoFocus
            />
          </div>
          <Button type="submit" className="w-full" disabled={loading || !token.trim()}>
            {loading && <Loader2 className="w-4 h-4 mr-2 animate-spin" />}
            Sign In
          </Button>
        </form>
      </Card>
    </div>
  );
}
