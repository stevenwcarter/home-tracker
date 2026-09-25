import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import eslint from 'vite-plugin-eslint';
import viteTsconfigPaths from 'vite-tsconfig-paths';
import tailwindcss from '@tailwindcss/vite';

const PROXY_ENDPOINT = 'http://localhost:7008';
const proxied = { target: PROXY_ENDPOINT, changeOrigin: true, secure: false };

export default defineConfig(() => ({
  build: { outDir: 'build' },
  test: {
    globals: true,
    environment: 'jsdom',
    setupFiles: ['./setupVitest.ts'],
    coverage: {
      reporter: ['text', 'html', 'cobertura', 'lcov', 'json-summary'],
      exclude: ['**/node_modules/**', '**/build/**', '**/*.js', 'src/main.tsx'],
    },
  },
  plugins: [react(), eslint(), viteTsconfigPaths(), tailwindcss()],
  server: {
    watch: { ignored: ['coverage', 'build'] },
    proxy: {
      '/graphql': proxied,
      '/graphiql': proxied,
      '/attachments/': proxied,
      '/api/': proxied,
    },
  },
}));
