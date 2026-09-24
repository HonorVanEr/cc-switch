import path from "node:path";
import { defineConfig, type Plugin } from "vite";
import react from "@vitejs/plugin-react";
import { codeInspectorPlugin } from "code-inspector-plugin";
import { transformSync } from "@babel/core";

function legacySyntaxPlugin(): Plugin {
  return {
    name: "legacy-syntax",
    enforce: "post",
    // dev 阶段兼容旧 JS 引擎（WebKitGTK 2.30.4，JavaScriptCore ≈ Safari 13.1）：
    // esbuild 源码 transform 不会降级 `export * as X from`（ES2020），旧引擎解析报错导致白屏。
    // 机械转成等价的 `import * as X` + `export { X }`（旧引擎支持），其余语法交给 esbuild target 处理。
    apply: "serve",
    transform(code, id) {
      if (!id.endsWith(".ts") && !id.endsWith(".tsx") && !id.endsWith(".js") && !id.endsWith(".jsx")) return;
      if (!/export\s+\*\s+as\s+[A-Za-z_$]/.test(code)) return;
      const out = code.replace(
        /export\s+\*\s+as\s+([A-Za-z_$][\w$]*)\s+from\s+(["'][^"']+["'])\s*;?/g,
        "import * as $1 from $2; export { $1 };",
      );
      return out === code ? undefined : { code: out, map: null };
    },
    transformIndexHtml() {},
  };
}

function babelPrivateFieldsBuildPlugin(): Plugin {
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
    command === "serve" ? legacySyntaxPlugin() : babelPrivateFieldsBuildPlugin(),
  ].filter(Boolean),
  base: "./",
  esbuild: {
    // 单一 target 而非多 target：esbuild 对私有类字段（#foo）的降级仅在 target 低于 chrome82/safari15 时启用。
    // WebKitGTK 2.30.4 的 JavaScriptCore ≈ Safari 13.1，不支持私有字段，需降级。
    // 注意：不可混入 es2018 等旧 target——会触发 esbuild 尝试降级解构语法而报错。
    // 顶层 esbuild.target 作用于 dev 运行时内联的源码/依赖 transform
    target: ["chrome80"],
  },
  optimizeDeps: {
    // 关键：预构建产物（node_modules/.vite/deps/*.js）由独立 esbuild 调用生成，
    // 不受顶层 esbuild.target 控制，必须用 optimizeDeps.esbuildOptions.target
    // 才能让私有类字段在预构建阶段被降级（旧 JS 引擎白屏的根源）。
    esbuildOptions: {
      target: ["chrome80"],
    },
  },
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