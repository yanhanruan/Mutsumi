import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'

// https://vite.dev/config/
export default defineConfig({
  plugins: [vue()],
  server: {
    // Keep this in lockstep with `tauri.conf.json > build.devUrl`. Without
    // strictPort Vite silently picks another port while Tauri keeps loading the
    // configured one, which can display a different local project by accident.
    host:       '127.0.0.1',
    port:       1420,
    strictPort: true,
  },
  build: {
    // Tauri's WebView2 on Windows is always a recent Chromium build — targeting
    // 'esnext' skips all transpilation and polyfills, shrinking the JS bundle.
    target:             'esnext',
    minify:             true,        // esbuild minify (default, made explicit)
    cssMinify:          true,        // minify CSS
    reportCompressedSize: false,     // skip gzip measurement — speeds up build
  },
})
