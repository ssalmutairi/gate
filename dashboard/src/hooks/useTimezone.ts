import { useState, useCallback } from 'react';

const STORAGE_KEY = 'gate-timezone';
const DEFAULT_TZ = 'Asia/Riyadh';

export const TIMEZONES = [
  'Asia/Riyadh',
  'UTC',
  'America/New_York',
  'America/Chicago',
  'America/Denver',
  'America/Los_Angeles',
  'Europe/London',
  'Europe/Paris',
  'Europe/Berlin',
  'Asia/Dubai',
  'Asia/Kolkata',
  'Asia/Singapore',
  'Asia/Tokyo',
  'Asia/Shanghai',
  'Australia/Sydney',
  'Pacific/Auckland',
] as const;

export type Timezone = (typeof TIMEZONES)[number] | string;

export function useTimezone() {
  const [timezone, setTimezoneState] = useState<string>(() => {
    return localStorage.getItem(STORAGE_KEY) || DEFAULT_TZ;
  });

  const setTimezone = useCallback((tz: string) => {
    localStorage.setItem(STORAGE_KEY, tz);
    setTimezoneState(tz);
  }, []);

  return { timezone, setTimezone };
}
