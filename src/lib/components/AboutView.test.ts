import { render, screen } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';
import pkg from '../../../package.json';
import AboutView from './AboutView.svelte';

describe('AboutView', () => {
  it('describes the app with the one description kept in package.json', () => {
    render(AboutView);
    expect(screen.getByText(pkg.description)).toBeTruthy();
  });
});
