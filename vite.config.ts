import path from "node:path";
import { defineConfig, type Plugin } from "vite";
import react from "@vitejs/plugin-react";
import { codeInspectorPlugin } from "code-inspector-plugin";
import { transformSync } from "@babel/core";

function babelPrivateFieldsPlugin(): Plugin {
  return {
    name: "babel-private-fields",
    enforce: "post",
    apply: "build",
    generateBundle(_, bundle) {
      for (const [fileName, chunk] of Object.entries(bundle)) {
        if (chunk.type === "chunk" && fileName.endsWith(".js")) {
          try {
            const result = transformSync(chunk.code, {
              presets: [
                ["@babel/preset-env", { targets: { chrome: "80", safari: "14" }, modules: false }],
              ],
              plugins: [
                "@babel/plugin-transform-private-methods",
                "@babel/plugin-transform-class-properties",
                "@babel/plugin-transform-private-property-in-object",
              ],
              filename: fileName,
              compact: false,
              sourceType: "module",
            });
            if (result && result.code) {
              chunk.code = result.code;
            }
          } catch (e: any) {
            if (!e.message?.includes("private") && !e.message?.includes("#")) {
              throw e;
            }
          }
          
          chunk.code = chunk.code.replace(/\binset-0\b/g, "top-0 left-0 right-0 bottom-0");
        }
      }
    },
  };
}

export default defineConfig(({ command }) => ({
  root: "src",
  plugins: [
    command === "serve" &&
      codeInspectorPlugin({
        bundler: "vite",
      }),
    react(),
    babelPrivateFieldsPlugin(),
  ].filter(Boolean),
  base: "./",
  build: {
    outDir: "../dist",
    emptyOutDir: true,
  },
  server: {
    port: 3000,
    strictPort: true,
  },
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
      "framer-motion": path.resolve(__dirname, "./src/lib/framer-motion-stub.tsx"),
    },
  },
  clearScreen: false,
  envPrefix: ["VITE_", "TAURI_"],
}));