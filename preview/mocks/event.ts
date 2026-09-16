import { listeners, startTicker } from './core';

let started = false;

export async function listen<T>(
  name: string,
  handler: (event: { payload: T }) => void
): Promise<() => void> {
  listeners.set(name, handler as (e: { payload: unknown }) => void);
  if (!started) {
    started = true;
    startTicker();
  }
  return () => listeners.delete(name);
}

export async function emit() {}
export async function once() {
  return () => {};
}
