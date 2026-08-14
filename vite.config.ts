import { defineConfig, type Plugin } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import path from "path";
import fs from "fs";

/**
 * Bundle budget gate: fail the production build when any single JS chunk
 * exceeds its byte budget, so size regressions surface in CI instead of
 * accumulating silently. Budgets sit ~10% above current sizes — raise them
 * deliberately (with a reason) when a feature legitimately needs the space.
 */
function bundleBudget(budgets: { pattern: RegExp; maxBytes: number; label: string }[]): Plugin {
  return {
    name: "omnix-bundle-budget",
    apply: "build",
    closeBundle() {
      const dir = path.resolve(__dirname, "dist/assets");
      if (!fs.existsSync(dir)) return;
      const failures: string[] = [];
      for (const file of fs.readdirSync(dir)) {
        if (!file.endsWith(".js")) continue;
        const size = fs.statSync(path.join(dir, file)).size;
        // First matching budget wins (order: specific → catch-all).
        const budget = budgets.find(({ pattern }) => pattern.test(file));
        if (budget && size > budget.maxBytes) {
          failures.push(`${file} (${(size / 1024).toFixed(0)}KB) 超出 ${budget.label} 预算 ${(budget.maxBytes / 1024).toFixed(0)}KB`);
        }
      }
      if (failures.length) {
        throw new Error(`bundle budget exceeded:\n  ${failures.join("\n  ")}`);
      }
    },
  };
}

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [
    react(),
    tailwindcss(),
    bundleBudget([
      { pattern: /^index-/, maxBytes: 480_000, label: "主包" },
      { pattern: /^vendor-/, maxBytes: 150_000, label: "vendor 块" },
      { pattern: /./, maxBytes: 160_000, label: "懒加载块" },
    ]),
  ],

  // Tauri v2: use relative paths so assets resolve correctly
  // under the tauri:// protocol in production builds
  base: "./",

  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    // HMR 走 1430，**不是 Tauri 模板给的 1421**——1421 是 OMNIX 网关自己的
    // 默认端口（`DEFAULT_PROXY_PORT`，散布在后端十几处）。设了 TAURI_DEV_HOST
    // 做局域网/手机调试时，两边会去抢同一个端口：谁先起来谁占住，另一个静默
    // 起不来。网关那个端口动不了（手机配对 URL、MCP 地址、反向隧道都写着它），
    // 所以让 dev server 让开。
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1430,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**", "**/scratch/**", "**/.claude/worktrees/**"],
    },
  },

  // 后台任务会在 `.claude/worktrees/` 下开工作副本——那里有一整份仓库拷贝。
  // 不排掉的话 vitest 会把副本里的测试也跑一遍：数量翻倍，而且一个过期的
  // 副本可能为早就改掉的代码报「通过」。
  test: {
    exclude: ["**/node_modules/**", "**/dist/**", "**/.claude/worktrees/**"],
  },

  // 4. Split large dependencies into separate chunks for better caching & lazy loading
  build: {
    rollupOptions: {
      output: {
        manualChunks: {
          "vendor-d3": ["d3"],
          "vendor-radix": [
            "@radix-ui/react-checkbox",
            "@radix-ui/react-dialog",
            "@radix-ui/react-label",
            "@radix-ui/react-select",
            "@radix-ui/react-slot",
            "@radix-ui/react-switch",
          ],
        },
      },
    },
  },
}));
