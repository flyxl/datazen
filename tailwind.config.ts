import type { Config } from 'tailwindcss';

export default {
  darkMode: 'class',
  content: ['./index.html', './src/**/*.{js,ts,jsx,tsx}', './packages/drivers/**/ui/**/*.{js,ts,jsx,tsx}'],
  theme: {
    extend: {
      fontFamily: {
        sans: ['var(--font-sans)'],
        mono: ['var(--font-mono)'],
      },
      colors: {
        surface: {
          DEFAULT: 'var(--c-surface)',
          alt: 'var(--c-surface-alt)',
          raised: 'var(--c-surface-raised)',
          inset: 'var(--c-surface-inset)',
        },
        edge: {
          DEFAULT: 'var(--c-edge)',
        },
        fg: {
          DEFAULT: 'var(--c-fg)',
          secondary: 'var(--c-fg-secondary)',
          muted: 'var(--c-fg-muted)',
        },
        titlebar: {
          DEFAULT: 'var(--c-titlebar)',
        },
        accent: {
          DEFAULT: 'var(--c-accent)',
        },
        success: {
          DEFAULT: 'var(--c-success)',
        },
        warning: {
          DEFAULT: 'var(--c-warning)',
        },
        danger: {
          DEFAULT: 'var(--c-danger)',
        },
      },
    },
  },
} satisfies Config;
