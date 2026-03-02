import { createContext, useContext } from 'react';
import { type ThemeId, THEMES, DEFAULT_THEME, STORAGE_KEY } from '../lib/themes';

export interface ThemeContextValue {
  theme: ThemeId;
  setTheme: (id: ThemeId) => void;
}

export const ThemeContext = createContext<ThemeContextValue>({
  theme: DEFAULT_THEME,
  setTheme: () => {},
});

// All CSS property names that dark themes may set
const ALL_PROPS = Object.keys(THEMES['github-dark'].colors);

export function applyTheme(id: ThemeId) {
  const theme = THEMES[id];
  if (!theme) return;

  const style = document.documentElement.style;

  // Clear all overrides first so light theme falls back to @theme defaults
  for (const prop of ALL_PROPS) {
    style.removeProperty(prop);
  }

  // Apply new overrides
  for (const [prop, value] of Object.entries(theme.colors)) {
    style.setProperty(prop, value);
  }

  // Toggle .dark class for dark: variant support
  document.documentElement.classList.toggle('dark', theme.isDark);

  // Persist
  localStorage.setItem(STORAGE_KEY, id);
}

export function getStoredTheme(): ThemeId {
  const stored = localStorage.getItem(STORAGE_KEY);
  if (stored && stored in THEMES) return stored as ThemeId;
  return DEFAULT_THEME;
}

export function useTheme() {
  return useContext(ThemeContext);
}
