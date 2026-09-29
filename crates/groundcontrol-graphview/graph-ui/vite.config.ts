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
          let html = fs.readFileSync(indexPath, 'utf8');
          // Normalize to LF line endings so the generated single-file bundle
          // is strictly byte-deterministic across Windows, macOS, and Linux CI.
          html = html.replace(/\r\n/g, '\n');
          fs.writeFileSync(targetPath, html, 'utf8');
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
