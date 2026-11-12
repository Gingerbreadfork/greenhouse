import { fireEvent, render, screen } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';
import Check from './Check.svelte';

describe('Check', () => {
  it('reports a change and keeps it', async () => {
    const seen: boolean[] = [];
    render(Check, { checked: true, label: 'A file', onchange: (on: boolean) => seen.push(on) });
    const box = screen.getByRole<HTMLInputElement>('checkbox');

    await fireEvent.click(box);

    expect(seen).toEqual([false]);
    expect(box.checked).toBe(false);
  });

  it('goes back when the change is turned down', async () => {
    render(Check, { checked: true, label: 'The only file', onchange: () => false });
    const box = screen.getByRole<HTMLInputElement>('checkbox');

    await fireEvent.click(box);

    expect(box.checked).toBe(true);
  });
});
