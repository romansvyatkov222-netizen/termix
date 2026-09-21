/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{svelte,ts}"],
  darkMode: "class",
  theme: {
    extend: {
      colors: {
        base: "var(--bg-base)",
        panel: "var(--bg-panel)",
        elevated: "var(--bg-elevated)",
        line: "var(--border)",
        accent: "var(--accent)",
        accentdim: "var(--accent-dim)",
        ok: "var(--ok)",
        warn: "var(--warn)",
        danger: "var(--danger)",
        txt: "var(--text)",
        sub: "var(--text-sub)",
        faint: "var(--text-faint)",
      },
      borderRadius: {
        DEFAULT: "8px",
        lg: "12px",
      },
    },
  },
  plugins: [],
};
