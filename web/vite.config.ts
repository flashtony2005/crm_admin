import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import { TanStackRouterVite } from '@tanstack/router-plugin/vite'
import tailwindcss from '@tailwindcss/vite'

export default defineConfig({
  plugins: [
    TanStackRouterVite({ autoCodeSplitting: true }),
    react(),
    tailwindcss(),
  ],
  server: {
    port: 5188,
    proxy: {
      '/api': {
        target: 'http://127.0.0.1:8088',
        changeOrigin: true,
      },
    },
  },
})
