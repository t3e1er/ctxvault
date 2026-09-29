import { defineConfig } from 'vite';
import { viteSingleFile } from 'vite-plugin-singlefile';
import fs from 'node:fs';
import path from 'node:path';

export default defineConfig({
  plugins: [
    viteSingleFile(),
    {
      name: 'rename-to-embedded-dashboard',
      closeBundle() {
        const outDir = path.resolve(__dirname, '../src');
        const indexPath = path.join(outDir, 'index.html');
        const targetPath = path.join(outDir, 'embedded_dashboard.html');
        if (fs.existsSync(indexPath)) {
          fs.copyFileSync(indexPath, targetPath);
          fs.unlinkSync(indexPath);
          console.log(`[vite] Bundled single-file UI into ${targetPath}`);
        }
      },
    },
  ],
  server: {
    port: 5173,
    proxy: {
      '/api': {
        target: 'http://localhost:9091',
        changeOrigin: true,
      },
    },
  },
  build: {
    outDir: '../src',
    emptyOutDir: false,
    target: 'esnext',
  },
});
