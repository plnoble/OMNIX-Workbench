











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

### ❌ 坑点 4: UI 工作台壳层把所有功能堆在首页或左侧栏，导致用户不知道从何开始。
* **危险模式/命令**: 默认入口暴露 Workbench/Runs/Plan/Resources/Labs/Settings 等多个面板；把全局功能全部塞进左侧主目录。
* **安全修复方案**: 先确认用户进入软件后的第一动作，并让默认首屏只服务这个动作。高频核心入口放顶部横向导航，低频资源/实验功能放可配置应用宫格；左侧栏只显示当前上下文，如对话、工作区、团队会话。
* **相关标签**: `ui,product-shell,navigation,workbench,ux`

<!--- OMNIX MEMORY END --->
