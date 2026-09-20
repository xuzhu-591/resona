import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
export default defineConfig({
  plugins: [react()],
  server: { port: 1420, strictPort: true, host: "127.0.0.1" },
  clearScreen: false,
  build: {
    rollupOptions: {
      output: {
        manualChunks(id) {
          if (id.includes("node_modules/echarts")) return "charts";
          if (id.includes("node_modules/zrender")) return "chart-renderer";
          if (id.includes("node_modules/react")) return "react";
        },
      },
    },
  },
});
