import { useState, useEffect, type ReactNode } from 'react';
import { type ThemeId } from '../lib/themes';
import { ThemeContext, applyTheme, getStoredTheme } from '../hooks/useTheme';

export default function ThemeProvider({ children }: { children: ReactNode }) {
  const [theme, setThemeState] = useState<ThemeId>(getStoredTheme);

  useEffect(() => {
    applyTheme(theme);
  }, []);

  const setTheme = (id: ThemeId) => {
    applyTheme(id);
    setThemeState(id);
  };

  return (
    <ThemeContext.Provider value={{ theme, setTheme }}>
      {children}
    </ThemeContext.Provider>
  );
}
