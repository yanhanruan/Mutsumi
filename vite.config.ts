import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'

// https://vite.dev/config/
export default defineConfig({
  plugins: [vue()],
  build: {
    // Tauri's WebView2 on Windows is always a recent Chromium build — targeting
    // 'esnext' skips all transpilation and polyfills, shrinking the JS bundle.
    target:             'esnext',
    minify:             true,        // esbuild minify (default, made explicit)
    cssMinify:          true,        // minify CSS
    reportCompressedSize: false,     // skip gzip measurement — speeds up build
  },
})
