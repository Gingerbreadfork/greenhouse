import { fireEvent, render, screen } from '@testing-library/svelte';
import { tick } from 'svelte';
import { beforeAll, describe, expect, it, vi } from 'vitest';
import Settings from './Settings.svelte';
import { store } from '../store.svelte';

beforeAll(() => store.init());

describe('Settings', () => {
  it('offers the network interface picker on Linux only', () => {
    store.platform = 'linux';
    const linux = render(Settings);
    expect(screen.queryByLabelText('Network interface')).not.toBeNull();
    linux.unmount();

    store.platform = 'windows';
    const windows = render(Settings);
    expect(screen.queryByLabelText('Network interface')).toBeNull();
    windows.unmount();
  });

  it('takes the disk space to keep free in gigabytes', async () => {
    render(Settings);
    const field = screen.getByLabelText<HTMLInputElement>('Gigabytes of disk space to keep free');
    expect(field.value).toBe('1');

    await fireEvent.change(field, { target: { value: '2.5' } });
    await vi.waitFor(() => expect(store.settings?.min_free_space_mb).toBe(2560));

    // An amount that is not a whole number of megabytes reads back as typed.
    await fireEvent.change(field, { target: { value: '0.3' } });
    await vi.waitFor(() => expect(store.settings?.min_free_space_mb).toBe(307));
    await tick();
    expect(field.value).toBe('0.3');

    await fireEvent.change(field, { target: { value: '1e12' } });
    await vi.waitFor(() => expect(store.settings?.min_free_space_mb).toBe(0xffff_ffff));

    await fireEvent.change(field, { target: { value: '' } });
    await vi.waitFor(() => expect(store.settings?.min_free_space_mb).toBe(0));
  });
});
