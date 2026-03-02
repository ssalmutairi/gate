import { useState, useEffect, useCallback, type ReactNode } from 'react';
import { AuthContext } from '../hooks/useAuth';
import LoginPage from '../pages/LoginPage';
import { Loader2 } from 'lucide-react';
import axios from 'axios';

type AuthState = 'loading' | 'authenticated' | 'unauthenticated';

export default function AuthProvider({ children }: { children: ReactNode }) {
  const [token, setToken] = useState<string | null>(null);
  const [state, setState] = useState<AuthState>('loading');

  const logout = useCallback(() => {
    localStorage.removeItem('admin_token');
    setToken(null);
    setState('unauthenticated');
  }, []);

  const login = useCallback((newToken: string) => {
    localStorage.setItem('admin_token', newToken);
    setToken(newToken);
    setState('authenticated');
  }, []);

  // Validate stored token on mount
  useEffect(() => {
    const stored = localStorage.getItem('admin_token');
    if (!stored) {
      setState('unauthenticated');
      return;
    }

    axios
      .get('/admin/health', { headers: { 'X-Admin-Token': stored } })
      .then(() => {
        setToken(stored);
        setState('authenticated');
      })
      .catch(() => {
        localStorage.removeItem('admin_token');
        setState('unauthenticated');
      });
  }, []);

  // Listen for 401 logout events from the api interceptor
  useEffect(() => {
    const handler = () => logout();
    window.addEventListener('auth:logout', handler);
    return () => window.removeEventListener('auth:logout', handler);
  }, [logout]);

  if (state === 'loading') {
    return (
      <div className="min-h-screen flex items-center justify-center bg-muted/30">
        <Loader2 className="w-8 h-8 animate-spin text-muted-foreground" />
      </div>
    );
  }

  if (state === 'unauthenticated') {
    return <LoginPage onLogin={login} />;
  }

  return (
    <AuthContext.Provider value={{ token, isAuthenticated: true, login, logout }}>
      {children}
    </AuthContext.Provider>
  );
}
