# 发版流程（CI 自动 + 应用内更新）

OMNIX 用 `tauri-plugin-updater` 做应用内更新，用 GitHub Actions（`.github/workflows/release.yml`
里的 `tauri-apps/tauri-action`）自动打包、签名、建 Release、生成 `latest.json`。用户端会自动检测
并弹窗更新。

## 一次性设置（只做一次）

CI 打包时要用**更新签名私钥**给安装包签名。私钥已在本机生成：

- 私钥：`~/.tauri/omnix-updater.key`（**保密！不要提交、不要外发**；`.gitignore` 已忽略 `*.key`）
- 公钥：`~/.tauri/omnix-updater.key.pub`（已写进 `src-tauri/tauri.conf.json` 的 `plugins.updater.pubkey`）

把私钥设为仓库 Secret（用 GitHub CLI，在项目根目录跑）：

```bash
gh secret set TAURI_SIGNING_PRIVATE_KEY < "$HOME/.tauri/omnix-updater.key"
gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD --body ""
```

（私钥是无密码生成的，所以密码 Secret 留空。）也可在 GitHub 仓库 Settings → Secrets and variables →
Actions 里手动添加这两个。

> ⚠️ **备份好私钥**。丢了它就再也发不出「能被老版本接受的更新」，只能让用户重装。

## 每次发版

1. **升版本号**（四处保持一致）：`package.json`、`src-tauri/Cargo.toml`、`src-tauri/tauri.conf.json`、
   `src-tauri/Cargo.lock`（omnix-workbench 条目）。
2. **提交并推送到 master**（正常的 release commit）。
3. **打 tag 并推 tag**（tag 名 = `v<版本>`，要和上面版本号一致）：
   ```bash
   git tag v0.7.0
   git push origin v0.7.0
   ```
4. CI 自动完成：打包 MSI/NSIS → 用私钥签名 → 建 GitHub Release `v0.7.0` → 上传安装包 +
   `.sig` + **`latest.json`**。
5. 已装 OMNIX 的用户下次启动（或点「检查更新」）就会看到更新弹窗。

无需再本机 `tauri build` 手动传包——tag 一推，CI 全包了。

## 应用内更新怎么触发

- **启动自动检测**：静默 `check()`，有新版就弹窗（版本 + 更新日志 + 下载进度 + 立即更新并重启）。
- **手动检查**：仪表盘「软件更新」卡片的「检查更新」按钮（任意处 `window.dispatchEvent(new
  Event("omnix:check-updates"))` 也能触发）。
- **稍后提醒**：点「稍后」后右下角挂一个「新版本可更新」小胶囊，随时点开。

## 注意事项

- **首个带 updater 的版本要手动装一次**。现有 0.6.0 没有 updater，升不上去；把第一个带 updater 的
  版本（如 0.7.0）手动装上，之后才会自动更新。
- **更新走 NSIS**（`-setup.exe`），MSI 留给首次直接下载。`installMode: passive` 静默升级。
- **Tauri 更新签名 ≠ Windows 代码签名（Authenticode）**。前者保证更新包不被篡改（已做）；后者才能
  去掉安装时 SmartScreen「未知发布者」警告，需要买证书（几百刀/年）。自用可先忽略，安装时点
  「更多信息 → 仍要运行」。
- `latest.json` 的 endpoint 固定为
  `https://github.com/plnoble/OMNIX-Workbench/releases/latest/download/latest.json`，永远指向最新 Release。
