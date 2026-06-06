
<!--- OMNIX MEMORY START --->
## 🧠 OMNIX Anti-Failure Guidelines & Memory Bank
以下是历史项目踩坑事故记录与规约，请在此工作区内严加防范，避免重犯相同错误：

### ❌ 坑点 1: 跨域请求中 credentials 与 Origin 冲突导致预检拦截。
* **危险模式/命令**: `fetch(url, { credentials: 'include', mode: 'cors' })`
* **安全修复方案**: 当请求设置 credentials 为 include 时，后端 CORS 响应头 Access-Control-Allow-Origin 不能设为通配符 *，必须指定明确的域名 Origin。
* **相关标签**: `cors,fetch,credentials,web`

### ❌ 坑点 2: Tokio 线程手动锁死：在 async fn 内阻塞等待 sync 互斥锁发生 panic 死锁。
* **危险模式/命令**: `std::sync::MutexGuard across await point`
* **安全修复方案**: 在异步 Task 跨 await 时不能持有 std::sync::MutexGuard，否则会导致 Send 校验失败或死锁。必须使用 tokio::sync::Mutex 或者在 await 前显式释放锁作用域。
* **相关标签**: `tokio,lock,deadlock,async`

### ❌ 坑点 3: Git 强制覆写推送导致公共代码库提交日志被覆盖损坏。
* **危险模式/命令**: `git push -f`
* **安全修复方案**: 在多人协作仓库中绝不能执行 git push -f。强制更新必须通过分支审批 PR，或使用 --force-with-lease 安全锁推送。
* **相关标签**: `git,push,deploy,safety`

### ❌ 坑点 4: 界面排版重叠、文字折行溢出及窗口化布局混乱。
* **危险模式/原因**: 
  1. 标题或标签使用固定容器高度（如 `height: 70px`）且未设置行高 `line-height`，在小分辨率或折行时会导致文字上下重叠或溢出。
  2. Flex 纵向容器中，卡片设置了 `flex-grow: 1` 和 `minHeight: 0`，导致在小屏幕或窗口模式下高度被强行压缩至极小，内部未包裹的子元素溢出，与下方的相邻卡片发生严重遮挡与重叠。
  3. 静态设置固定的网格列数（如 `repeat(3, 1fr)`），在窗口模式、分辨率变化时无法自适应，导致文字超出格子边界。
* **安全修复方案**:
  1. 标题类容器使用 `min-height` 替代固定 `height`，且设置 `height: auto`，并为标题文字指定明晰的行高（如 `line-height: 1.4`），确保折行时自适应撑开不重叠。
  2. 对包含复杂交互的组件（如代码编辑器、拓扑图卡片），强制设置合理的高度下限（如 `min-height: 550px`）或设置 `flex-shrink: 0`，并在其滚动的外部父容器配置 `overflow-y: auto`，允许长页面自然向下滚动。
  3. 卡片网格一律使用响应式自适应列宽：`repeat(auto-fit, minmax(240px, 1fr))`，确保在窄屏下能自动降列，避免内容超出或大小不一。
* **相关标签**: `css,layout,flexbox,responsive,reflow`

<!--- OMNIX MEMORY END --->
