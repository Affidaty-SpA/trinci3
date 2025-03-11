import { fileURLToPath, URL } from 'node:url'
import { defineConfig, loadEnv } from 'vite'
import vue from '@vitejs/plugin-vue'
import inject from '@rollup/plugin-inject'
import { nodePolyfills } from 'vite-plugin-node-polyfills'


export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, process.cwd(), '')

  return {
    plugins: [
      vue(),
      nodePolyfills()
    ],
    optimizeDeps: {
    },
    build: {
      rollupOptions: {
        plugins: [inject({ Buffer: ['buffer', 'Buffer'] })],
      },
      target: ['es2022']
    },
    base: '/',
    resolve: {
      alias: {
        '@': fileURLToPath(new URL('./src', import.meta.url))
      }
    },
    server: {
      port: 3000,
      proxy: {
        "^/mining": {
          target: env.VITE_REST_ENDPOINT,
          changeOrigin: true,
          secure: false
        },
        "^/api": {
          target: env.VITE_API_PROXI,
          changeOrigin: true,
          secure: false
        },
        // Proxying websockets or socket.io
        '^/ws': {
          target: 'ws://127.0.0.1:7000/ws',
          ws: true,
          changeOrigin: true,
          secure: false,
        }
      },
    },
  }
})
