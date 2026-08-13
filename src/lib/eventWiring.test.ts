import fs from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";

/**
 * Tauri 事件的两端必须都在。
 *
 * 这条守的是这一版撞到两次的同一类 bug——**发送方存在，接收方不存在，而且没有
 * 任何东西会报错**：
 *
 * 1. `omnix-dev-status-change`：主窗口发的字段名和悬浮坞读的完全不一样，坞里
 *    常年读到 undefined。
 * 2. `omnix-notification`：`hooks.rs` 的 notify 动作和 `send_desktop_notification`
 *    都在发，前端**一个监听方都没有**。钩子面板里「桌面通知」还是默认动作，
 *    跑完还会往运行记录里写「已发送通知」——用户看见回执，屏幕上什么都没有。
 *
 * `emit` 的载荷是 unknown、`listen<T>` 只是断言，所以 TypeScript 一路沉默。
 * 只能在这里扫源码。
 */

const ROOT = path.resolve(__dirname, "../..");

/** 后端 emit 但**故意**不需要前端监听的事件，加在这里并写明理由。 */
const NO_FRONTEND_LISTENER: Record<string, string> = {};

function readAll(dir: string, exts: string[]): string {
  let out = "";
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      if (entry.name === "node_modules" || entry.name === "target") continue;
      out += readAll(full, exts);
    } else if (exts.some((e) => entry.name.endsWith(e))) {
      out += fs.readFileSync(full, "utf8") + "\n";
    }
  }
  return out;
}

/** `.emit("name"` / `.emit_to(win, "name"` —— 允许参数跨行。 */
function emittedEvents(rust: string): Set<string> {
  const names = new Set<string>();
  const re = /\.emit(?:_to)?\s*\(\s*(?:[^,()"]+,\s*)?"([a-z0-9-]+)"/g;
  for (const m of rust.matchAll(re)) names.add(m[1]);
  return names;
}

/**
 * `listen("name"` / `listen<T>("name"`。
 *
 * 泛型里用 `[^()]*` 而不是 `[^>]*`：`listen<Record<string, unknown>>(...)` 有嵌套
 * 尖括号，按 `>` 截会在第一个 `>` 断掉，于是**漏掉这个监听方、误报成没人接**。
 * 我第一版就是这么写的，一跑就冤枉了 `qa-stream-done`。
 */
function listenedEvents(ts: string): Set<string> {
  const names = new Set<string>();
  const re = /listen\s*(?:<[^()]*>)?\s*\(\s*"([a-z0-9-]+)"/g;
  for (const m of ts.matchAll(re)) names.add(m[1]);
  return names;
}

describe("Tauri 事件接线", () => {
  it("后端发出的每个事件都有前端监听方", () => {
    const rust = readAll(path.join(ROOT, "src-tauri", "src"), [".rs"]);
    const ts = readAll(path.join(ROOT, "src"), [".ts", ".tsx"]);
    const emitted = emittedEvents(rust);
    const listened = listenedEvents(ts);

    // 先自检：扫得到东西才说明正则没写死
    expect(emitted.size).toBeGreaterThan(3);
    expect(listened.size).toBeGreaterThan(3);

    const orphans = [...emitted].filter(
      (name) => !listened.has(name) && !(name in NO_FRONTEND_LISTENER),
    );
    expect(orphans, `这些事件发出去没人接：${orphans.join(", ")}`).toEqual([]);
  });
});
