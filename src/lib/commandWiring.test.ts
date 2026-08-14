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
  "get_conversation_tasks",
  "get_mcp_presets",
  "get_output_style_prompt",
  "get_output_styles",
  "get_skill_content",
  "read_file_as_base64",
  "read_file_content_utf8",
  "save_skill_content",
  "scan_prompt_injection",
  "toggle_skill_active",
  "uninstall_agent_cli",
  "update_skill_profile",
];

function readFiles(dir: string, exts: string[]): string[] {
  const out: string[] = [];
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      if (entry.name === "node_modules" || entry.name === "target") continue;
      out.push(...readFiles(full, exts));
    } else if (exts.some((e) => entry.name.endsWith(e))) {
      out.push(full);
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
  // 排除测试文件：只被测试调到的命令不算「接上了」，而且本文件的注释里就写着
  // `invoke("name")` 这样的示例——扫进来会被下面「幽灵调用」那条当成真调用。
  const ts = readFiles(path.join(ROOT, "src"), [".ts", ".tsx"])
    .filter((f) => !/\.test\.tsx?$/.test(f))
    .map((f) => fs.readFileSync(f, "utf8"))
    .join("\n");
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

  /**
   * 反方向：`invoke` 的命令必须真的注册过。
   *
   * 前两条守的都是「后端有、前端没人用」——那只是死代码。这条守的是
   * **前端在调一个根本不存在的命令**，性质完全不同：**调到就是运行时报错**
   * （Tauri 抛 "command not found"），而 TypeScript 一个字都不会说，因为命令名
   * 只是个字符串。
   *
   * 不是假设：删掉文件版信箱那一轮只删了 Rust，`mailboxApi` 的三条 invoke 留在
   * 前端；`tokenEconomyApi` 五条、`eventBusApi` 两条同样如此。十条幽灵调用，
   * 前两道守卫一条都抓不到——因为它们只从「注册表」这一侧出发。
   */
  it("前端调用的每个命令都真的注册过", () => {
    const phantoms = [...invoked].filter((name) => !registered.has(name)).sort();
    expect(
      phantoms,
      `前端在调这些不存在的命令，调到就会运行时报错：${phantoms.join(", ")}\n` +
        `要么后端补上，要么把前端这段删掉。`,
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

/**
 * 第二道接缝：API 包装对象必须有组件在用。
 *
 * 上面那道只查「Rust 命令 → 有没有 `invoke`」。可是 `invoke` 就写在
 * `src/lib/api/*.ts` 的包装里，所以只要包装还在，命令就永远算「有人调」——哪怕
 * 从没有任何组件 import 过那个包装。整条链是死的，上面那道一个都抓不到。
 *
 * 这不是假设：`skillCompoundApi`（技能复利）当初就是这么躺着的，靠手工翻才发现；
 * `skillDagApi` 同样如此，它甚至通过了第一版守卫。量下来 85 个包装里有 21 个是
 * 这个状态，背后拖着 62 条 Rust 命令。
 */
const API_DIR = path.join(ROOT, "src", "lib", "api");
const BARREL = path.join(ROOT, "src", "lib", "tauri-api.ts");

/**
 * 已知没有组件调用方的包装。**只允许变短。**
 *
 * 和 KNOWN_ORPHANS 一样，这是待定夺的债，不是豁免：每一条要么接上界面、要么连同
 * 它背后的 Rust 命令一起删。新写的包装不许进这里——没有组件要用，就先别写。
 */
const KNOWN_UNUSED_APIS = [
  "activityApi",
  "agentExecApi",
  "apiPresetApi",
  "codeAnalysisApi",
  "configBackupApi",
  "healthCheckApi",
  "modelSyncApi",
  "notificationApi",
  "platformHealthApi",
  "skillDagApi",
  "workspaceGcApi",
  "yoloApi",
];

describe("API 包装接线", () => {
  const exported = new Set<string>();
  for (const file of readFiles(API_DIR, [".ts"])) {
    const src = fs.readFileSync(file, "utf8");
    for (const m of src.matchAll(/^export const ([a-zA-Z0-9_]+Api)\s*=/gm)) exported.add(m[1]);
  }

  // 消费方 = 除了 api/ 目录、桶文件、以及**测试文件**之外的所有前端源码。
  //
  // 桶只是 `export *` 转发，出现一次不代表有人用。测试文件更要排除，否则第一次跑
  // 就会被自己坑到：下面那份 KNOWN_UNUSED_APIS 清单本身就写在 `src/` 下的一个
  // `.ts` 里，21 个名字全在，于是每一个都被判成「有人用」——守卫把自己的豁免清单
  // 当成了调用方。只被测试用到的包装同样是死的。
  const consumers = readFiles(path.join(ROOT, "src"), [".ts", ".tsx"])
    .filter((f) => !f.startsWith(API_DIR) && f !== BARREL && !/\.test\.tsx?$/.test(f))
    .map((f) => fs.readFileSync(f, "utf8"))
    .join("\n");

  const isUsed = (name: string) => new RegExp(`\\b${name}\\b`).test(consumers);

  it("每个 API 包装都有组件在用", () => {
    expect(exported.size).toBeGreaterThan(50);

    const unused = [...exported]
      .filter((name) => !isUsed(name) && !KNOWN_UNUSED_APIS.includes(name))
      .sort();
    expect(
      unused,
      `这些 API 包装没有任何组件在用：${unused.join(", ")}\n` +
        `要么接上界面，要么连同背后的 Rust 命令一起删。不要加进 KNOWN_UNUSED_APIS。`,
    ).toEqual([]);
  });

  it("存量未用包装清单里没有过期条目", () => {
    const stale = KNOWN_UNUSED_APIS.filter(
      (name) => !exported.has(name) || isUsed(name),
    ).sort();
    expect(
      stale,
      `KNOWN_UNUSED_APIS 里这些条目已经不成立（包装没了，或已经有人用），请删掉：${stale.join(", ")}`,
    ).toEqual([]);
  });
});
