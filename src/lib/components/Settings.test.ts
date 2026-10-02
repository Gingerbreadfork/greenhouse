import { render, screen } from '@testing-library/svelte';
import { beforeAll, describe, expect, it } from 'vitest';
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
    render(Settings);
    expect(screen.queryByLabelText('Network interface')).toBeNull();
  });
});
