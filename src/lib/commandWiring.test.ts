import fs from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";

/**
 * 注册进 `generate_handler!` 的命令，前端必须真的调。
 *
 * 姊妹守卫是 `eventWiring.test.ts`（emit 有没有 listen），这条守的是另一半接缝：
 * **命令注册了、编译过了、测试绿了，但没有任何一行前端代码会调它**。Tauri 的
 * 命令表是宏展开出来的字符串，前端 `invoke("name")` 也是字符串，两边谁都不认识
 * 谁——所以 Rust 编译器和 TypeScript 一起沉默。
 *
 * 这轮就是这么挖出「技能复利系统」整套是死的：`record_skill_usage` 从来没被调用，
 * 于是 `success_count` / `priority_score` 恒为默认值，而两处 `ORDER BY
 * priority_score DESC` 在按一个常数排序。同一批里 `get_conversation_skills` 更甚
 * ——它把 conversation_id 查出来直接丢掉（`let _workspace`），返回的是全部技能。
 * 没有调用方，就没有人发现它名不副实。
 */

const ROOT = path.resolve(__dirname, "../..");

/**
 * 已知的存量孤儿：注册了但前端没有调用方。
 *
 * **这个清单只允许变短。** 它不是「豁免」，是一笔还没定夺的债——每一条要么接上
 * 界面，要么删掉。新增命令一律不许进这里：加不进调用方就说明这个命令还不该注册。
 */
const KNOWN_ORPHANS = [
  "apply_mcp_preset",
  "get_all_models_metadata",
  "get_all_skills",
  "get_conversation_tasks",
  "get_mcp_presets",
  "get_output_style_prompt",
  "get_output_styles",
  "get_skill_content",
  "read_file_as_base64",
  "read_file_content_utf8",
  "relock_skill",
  "save_skill_content",
  "scan_prompt_injection",
  "skill_lock_audit",
  "toggle_skill_active",
  "uninstall_agent_cli",
  "update_skill_profile",
];

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

/** `generate_handler![...]` 里的 `commands::name,`。 */
function registeredCommands(libRs: string): Set<string> {
  const names = new Set<string>();
  for (const m of libRs.matchAll(/^\s*commands::([a-z0-9_]+),\s*$/gm)) names.add(m[1]);
  return names;
}

/**
 * `invoke("name"` / `invoke<T>("name"`。
 *
 * 泛型用 `[^()]*` 而不是 `[^>]*`——`invoke<Array<{ a: string }>>(...)` 有嵌套尖
 * 括号，按 `>` 截会在第一个 `>` 断掉，把好好的调用方判成不存在。这个坑
 * `eventWiring.test.ts` 已经踩过一次，别再踩第二次。
 */
function invokedCommands(ts: string): Set<string> {
  const names = new Set<string>();
  for (const m of ts.matchAll(/invoke\s*(?:<[^()]*>)?\s*\(\s*"([a-z0-9_]+)"/g)) names.add(m[1]);
  return names;
}

describe("Tauri 命令接线", () => {
  const libRs = fs.readFileSync(path.join(ROOT, "src-tauri", "src", "lib.rs"), "utf8");
  const ts = readAll(path.join(ROOT, "src"), [".ts", ".tsx"]);
  const registered = registeredCommands(libRs);
  const invoked = invokedCommands(ts);

  it("注册的每个命令都有前端调用方", () => {
    // 先自检：扫得到东西才说明正则没写死
    expect(registered.size).toBeGreaterThan(100);
    expect(invoked.size).toBeGreaterThan(100);

    const orphans = [...registered]
      .filter((name) => !invoked.has(name) && !KNOWN_ORPHANS.includes(name))
      .sort();
    expect(
      orphans,
      `这些命令注册了但前端没人调：${orphans.join(", ")}\n` +
        `要么接上界面，要么从 generate_handler! 里删掉。不要加进 KNOWN_ORPHANS。`,
    ).toEqual([]);
  });

  it("存量孤儿清单里没有过期条目", () => {
    // 清单会烂：命令被删了、或者后来接上了，条目却留着。留着就等于把守卫的口子
    // 越开越大，所以让它自己报出来。
    const stale = KNOWN_ORPHANS.filter(
      (name) => !registered.has(name) || invoked.has(name),
    ).sort();
    expect(
      stale,
      `KNOWN_ORPHANS 里这些条目已经不成立（命令没了，或已经有调用方），请删掉：${stale.join(", ")}`,
    ).toEqual([]);
  });
});
