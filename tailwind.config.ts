import type { Config } from 'tailwindcss';

export default {
  darkMode: 'class',
  content: ['./index.html', './src/**/*.{js,ts,jsx,tsx}'],
  theme: {
    extend: {
      fontFamily: {
        sans: ['-apple-system', 'BlinkMacSystemFont', '"Segoe UI"', 'Roboto', '"Helvetica Neue"', 'Arial', '"PingFang SC"', '"Hiragino Sans GB"', '"Microsoft YaHei"', 'sans-serif'],
        mono: ['Menlo', 'Monaco', 'Consolas', '"Courier New"', 'ui-monospace', 'monospace'],
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
      },
    },
  },
} satisfies Config;
