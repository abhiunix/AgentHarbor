/** @type {import('tailwindcss').Config} */
export default {
  content: [
    "./index.html",
    "./src/**/*.{js,ts,jsx,tsx}",
  ],
  darkMode: 'class',
  theme: {
    extend: {
      colors: {
        app: {
          bg: '#0e0f13',
          sidebar: '#13141a',
          card: '#1a1b23',
          'card-hover': '#22232e',
          input: '#1a1b23',
          modal: '#16171f',
        },
        border: {
          DEFAULT: '#2a2b36',
          light: '#333446',
        },
        text: {
          primary: '#e8e9ed',
          secondary: '#9394a1',
          muted: '#5f6070',
        },
        accent: {
          blue: '#5b8af5',
          'blue-glow': 'rgba(91,138,245,.15)',
          green: '#34d399',
          'green-dim': 'rgba(52,211,153,.12)',
          purple: '#a78bfa',
          'purple-dim': 'rgba(167,139,250,.12)',
          orange: '#fb923c',
          'orange-dim': 'rgba(251,146,60,.12)',
          red: '#f87171',
          'red-dim': 'rgba(248,113,113,.12)',
          yellow: '#fbbf24',
          'yellow-dim': 'rgba(251,191,36,.12)',
          cyan: '#22d3ee',
          'cyan-dim': 'rgba(34,211,238,.12)',
          pink: '#f472b6',
          'pink-dim': 'rgba(244,114,182,.12)',
        },
      },
      fontFamily: {
        sans: ['DM Sans', '-apple-system', 'BlinkMacSystemFont', 'SF Pro Text', 'sans-serif'],
        mono: ['JetBrains Mono', 'monospace'],
      },
      borderRadius: {
        sm: '6px',
        md: '10px',
        lg: '14px',
      },
      boxShadow: {
        modal: '0 25px 60px rgba(0,0,0,.5), 0 0 0 1px rgba(255,255,255,.05)',
      },
      transitionDuration: {
        DEFAULT: '180ms',
      },
      width: {
        sidebar: '240px',
      },
      minWidth: {
        sidebar: '240px',
      },
    },
  },
  plugins: [],
}
