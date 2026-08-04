#!/usr/bin/env bash
#
# 坑点守卫 —— 把 CLAUDE.md 里记着的历史事故变成 CI 里真的拦得住的门。
#
# 为什么要有这个：那几条坑点是这个项目**自己踩过**的，代价真实发生过，然后被
# 写进了 CLAUDE.md。但写下来跟拦得住是两回事——只写不管，下次照样踩。
#
# ## 设计上最重要的一条：每条规矩交给判得动它的机制
#
# 第一版这个脚本用文本匹配扫全仓，结果自己撞进了它警告的那个坑：一口气报了
# 9 条，**全是误报**——本仓库的记忆库把这些危险模式当**数据**存着
# （db_schema.rs 的种子行、memory_recall.rs 的测试夹具、解释历史 bug 的注释）。
# 文本匹配分不清「做这件事的代码」和「描述这件事的文字」。误报两次，门就废了。
#
# 所以现在：
#   坑点1（CORS 通配 + 凭证）→ 文本匹配，但**只扫真正构造 CORS 的地方**，
#                              且跳过注释行；合理用法就地写 `坑点1-ack:` 说明
#   坑点2（跨 await 持锁）    → 不在这里管。交给 clippy::await_holding_lock，
#                              它做真正的作用域分析（见 ci.yml 单独那一步）
#   坑点3（强推公共分支）      → 只扫**自动化**（工作流 / 脚本 / npm scripts）。
#                              强推写进自动化才是真危险；数据库种子里的一句
#                              字符串不是。
#
# 退出码：0 = 干净，1 = 有命中（CI 挂）。

set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

fail=0
err() {
  echo "::error file=$1,line=$2::$3"
  fail=1
}

# 三行以内有 `坑点N-ack` 就算就地说明过了。
has_ack() {
  local file=$1 line=$2 start
  start=$(( line > 3 ? line - 3 : 1 ))
  sed -n "${start},${line}p" "$file" | grep -q "坑点$3-ack"
}

# ─────────────────────────────────────────────────────────────────────────
# 坑点 1：CORS 通配符 + 携带凭证
#
# CLAUDE.md：「当请求设置 credentials 为 include 时，后端 CORS 响应头
# Access-Control-Allow-Origin 不能设为通配符 *，必须指定明确的域名 Origin。」
#
# 不是一律禁止：只绑 127.0.0.1 时宽松 CORS 风险很低，有合理用法。所以要求
# **就地写明理由**——「这里为什么可以宽松」从口头判断变成留在代码里的判断。
# ─────────────────────────────────────────────────────────────────────────
# 只看后端真正构造 CORS 的文件；`grep -v '^\s*//'` 跳过解释历史 bug 的注释。
while IFS=: read -r file line _; do
  [ -z "${file:-}" ] && continue
  has_ack "$file" "$line" 1 || err "$file" "$line" \
    "坑点1：CorsLayer::permissive() 没有就地说明理由。若这里确实安全（例如只绑 loopback、且不使用 cookie 凭证），在它上方三行内加注释「坑点1-ack: <理由>」；否则改成显式 allow_origin 白名单。"
done < <(grep -rn "CorsLayer::permissive" src-tauri/src/proxy*.rs 2>/dev/null \
           | grep -vE ':[0-9]+:\s*//' || true)

# 前端侧：带凭证的跨域请求。只扫前端源码——后端那边出现这串字是记忆库数据。
while IFS=: read -r file line _; do
  [ -z "${file:-}" ] && continue
  has_ack "$file" "$line" 1 || err "$file" "$line" \
    "坑点1：请求带 credentials: 'include'。确认后端对应的 Access-Control-Allow-Origin 是明确域名而非 *，然后加注释「坑点1-ack: <理由>」。"
done < <(grep -rn "credentials:\s*['\"]include['\"]" src/ 2>/dev/null \
           | grep -vE ':[0-9]+:\s*(//|\*)' || true)

# ─────────────────────────────────────────────────────────────────────────
# 坑点 3：强制推送覆盖公共仓库历史
#
# CLAUDE.md：「在多人协作仓库中绝不能执行 git push -f。强制更新必须通过分支
# 审批 PR，或使用 --force-with-lease 安全锁推送。」
#
# 只扫自动化：谁把强推写进工作流或脚本，就等于把这条规矩交给了一个不会犹豫的
# 执行者。源码里作为字符串出现（记忆库种子、测试夹具）不是危险。
# ─────────────────────────────────────────────────────────────────────────
while IFS=: read -r file line rest; do
  [ -z "${file:-}" ] && continue
  case "$rest" in *--force-with-lease*) continue ;; esac
  has_ack "$file" "$line" 3 || err "$file" "$line" \
    "坑点3：自动化里出现了强制推送。绝不能强推公共分支；确需强更请用 --force-with-lease，或走 PR。"
done < <(git grep -nE "git +push +(-f\b|--force\b)" -- \
           '.github/**' 'scripts/**' 'package.json' \
           ':!.github/scripts/pitfall-guard.sh' 2>/dev/null || true)

if [ "$fail" -eq 0 ]; then
  echo "坑点守卫：通过（CLAUDE.md 记录的历史事故模式均未出现）"
else
  echo ""
  echo "坑点守卫：有命中。这些不是通用 lint，是这个项目真实踩过的坑——"
  echo "详见 CLAUDE.md 的「Anti-Failure Guidelines」。"
fi
exit "$fail"
