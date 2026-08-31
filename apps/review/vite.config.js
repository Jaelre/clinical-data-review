import { defineConfig } from 'vite'
import { fileURLToPath } from 'node:url'

const page = (name) => fileURLToPath(new URL(`./ui/${name}`, import.meta.url))

export default defineConfig({
  root: './ui',
  server: {
    port: 5173,
    host: '127.0.0.1',
    strictPort: true,
    hmr: {
      port: 5174
    }
  },
  build: {
    outDir: '../dist',
    emptyOutDir: true,
    rollupOptions: {
      input: {
        index: page('index.html'),
        patient: page('patient.html'),
        summary: page('summary.html')
      }
    }
  },
  clearScreen: false
})
