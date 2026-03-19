/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{js,ts,jsx,tsx}"],
  theme: {
    extend: {
      colors: {
        sidebar: "#0F1B2D",
        content: "#F9FAFB",
        pii: {
          name: "#3B82F6",
          address: "#10B981",
          date: "#F59E0B",
          financial: "#EF4444",
          id: "#8B5CF6",
          heuristic: "#F59E0B",
        },
      },
      fontFamily: {
        sans: ["Inter", "system-ui", "sans-serif"],
        mono: ["JetBrains Mono", "monospace"],
      },
    },
  },
  plugins: [],
};
