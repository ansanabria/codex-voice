import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import path from "node:path";

const productionContentSecurityPolicy = "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:; connect-src 'none'; base-uri 'none'; form-action 'none'";

export default defineConfig(({ command }) => ({
  plugins: [
    react(),
    tailwindcss(),
    ...(command === "build" ? [{
      name: "packaged-content-security-policy",
      transformIndexHtml: {
        order: "post" as const,
        handler: () => [{
          tag: "meta",
          attrs: { "http-equiv": "Content-Security-Policy", content: productionContentSecurityPolicy },
          injectTo: "head" as const
        }]
      }
    }] : [])
  ],
  resolve: { alias: { "@": path.resolve(__dirname, "./src") } },
  server: { host: "127.0.0.1", port: 5173, strictPort: true },
  base: "./"
}));
