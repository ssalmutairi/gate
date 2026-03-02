import { useQuery } from '@tanstack/react-query';
import { getHealth } from '../lib/api';
import { useTheme } from '../hooks/useTheme';
import { useAuth } from '../hooks/useAuth';
import { useTimezone, TIMEZONES } from '../hooks/useTimezone';
import { THEMES, type ThemeId } from '../lib/themes';
import { Card } from '../components/ui/card';
import { Label } from '../components/ui/label';
import { Button } from '../components/ui/button';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '../components/ui/select';
import { LogOut } from 'lucide-react';

export default function SettingsPage() {
  const { theme, setTheme } = useTheme();
  const { token, logout } = useAuth();
  const { timezone, setTimezone } = useTimezone();
  const health = useQuery({ queryKey: ['health'], queryFn: getHealth, staleTime: 60_000 });

  return (
    <div className="max-w-2xl">
      <h1 className="text-2xl font-bold mb-6">Settings</h1>

      <div className="space-y-6">
        {/* Appearance */}
        <Card className="p-6">
          <h2 className="text-lg font-semibold mb-4">Appearance</h2>
          <div className="space-y-1">
            <Label>Theme</Label>
            <Select value={theme} onValueChange={(v) => setTheme(v as ThemeId)}>
              <SelectTrigger className="w-[240px]">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {Object.values(THEMES).map((t) => (
                  <SelectItem key={t.id} value={t.id}>{t.label}</SelectItem>
                ))}
              </SelectContent>
            </Select>
            <p className="text-xs text-muted-foreground mt-1">
              Choose your preferred color theme for the dashboard.
            </p>
          </div>
        </Card>

        {/* Timezone */}
        <Card className="p-6">
          <h2 className="text-lg font-semibold mb-4">Timezone</h2>
          <div className="space-y-1">
            <Label>Display timezone</Label>
            <Select value={timezone} onValueChange={setTimezone}>
              <SelectTrigger className="w-[240px]">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {TIMEZONES.map((tz) => (
                  <SelectItem key={tz} value={tz}>{tz}</SelectItem>
                ))}
              </SelectContent>
            </Select>
            <p className="text-xs text-muted-foreground mt-1">
              All dates and times across the dashboard will use this timezone.
            </p>
          </div>
        </Card>

        {/* Authentication */}
        <Card className="p-6">
          <h2 className="text-lg font-semibold mb-4">Authentication</h2>
          <div className="space-y-3 text-sm">
            <div className="flex justify-between items-center">
              <span className="text-muted-foreground">Admin Token</span>
              <span className="font-mono">
                {token ? `${token.slice(0, 4)}${'*'.repeat(Math.min(token.length - 4, 12))}` : '—'}
              </span>
            </div>
            <Button variant="outline" size="sm" onClick={logout}>
              <LogOut className="w-4 h-4 mr-2" />
              Sign Out
            </Button>
          </div>
        </Card>

        {/* About */}
        <Card className="p-6">
          <h2 className="text-lg font-semibold mb-4">About</h2>
          <dl className="space-y-3 text-sm">
            <div className="flex justify-between">
              <dt className="text-muted-foreground">Application</dt>
              <dd className="font-medium">Gate API Gateway</dd>
            </div>
            <div className="flex justify-between">
              <dt className="text-muted-foreground">Version</dt>
              <dd className="font-mono">
                {health.data?.version ? `v${health.data.version}` : '—'}
              </dd>
            </div>
          </dl>
        </Card>
      </div>
    </div>
  );
}
