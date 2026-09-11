# OMNIX Workbench 软件审核与修改建议

- 审核日期：2026-09-10
- 项目：OMNIX Workbench（多 Agent 开发与协作工作台）
- 代码基线：`8f0b499`，版本 `0.36.0`；开始审核时 Git 工作区干净。
- 本次重点：模型平台新增/编辑、密钥保存与读取、数据库迁移、检查点恢复、预览和任务桌面的现状。
- 方法：检查上述关键路径源码、复现数据库错误、执行构建/测试/静态检查。不是所有功能的逐屏人工验收，也不是外部渗透测试。
- 状态约定：**已修复**指本次工作区源码已修改并验证；**待修复**指已给出代码依据，尚未实施。安装版不会因为源码修改而自动更新。

## 1. 审核结论

当前最直接的使用障碍是模型平台的保存 SQL 与数据库结构不一致。本次已修复该问题，同时将平台与首个密钥改为一次事务保存，统一模型发现/批量检测的密钥读取，并保留编辑平台时的禁用状态。

本报告记录 11 个问题：本次已修复 4 项，待修复 7 项；另提出 4 项任务桌面产品建议。

软件已经具备顶部导航、应用宫格、工作会话、计划、Git 检查点和文件预览等基础。任务桌面应复用这些能力，先解决状态归属和恢复，再增加布局与交付架。

后续优先级最高的是**旧密钥迁移可能丢失原值**和**检查点回退前备份失败仍继续恢复**。本次没有重写这些流程，也没有把测试通过等同于全部缺陷消失。

## 2. 验证结果

| 检查 | 本次结果 | 能证明的范围 |
|---|---|---|
| `npm run build` | 通过 | TypeScript 类型检查与 Vite 生产构建 |
| `npm test` | 11 个文件、78 项测试通过 | 仓库现有前端测试；不等于原生窗口点击验收 |
| `cargo test --lib --offline commands::platforms::save_tests` | 新增 7 项全部通过 | 原报错复现、平台保存、加密兼容、回滚、编辑保留与启动迁移后取 Key |
| `cargo test --lib --offline --quiet` | 681 通过，0 失败，7 ignored | 当前后端自动化测试；7 项忽略用例未运行 |
| `cargo clippy --lib --tests --offline --quiet -- -D warnings` | 通过 | Rust 错误/警告门禁 |
| `npm exec -- biome lint src --diagnostic-level=error` | 通过，检查 142 个文件 | 前端 error 级静态检查；不代表所有 warning 为零 |
| `git diff --check` | 通过 | 本次修改没有空白格式错误 |
| `npm run tauri -- build --no-bundle` | 通过，release 编译退出码 0 | 已生成 Windows EXE；未自动安装、发布或进行原生 UI 点击验收 |

构建仍有两条非阻塞提示：空的 `vendor-d3` chunk，以及 `sonner.tsx` 同时静态/动态导入。它们不是本次保存失败原因，可后续整理。

本次未连接用户的付费模型接口，未执行真实推理/计费请求，也未验证已安装旧版本界面的完整端到端流程。

## 3. 已修复的问题

### R01 · P1：模型平台保存触发 NOT NULL 约束

**现象**

```text
保存失败：NOT NULL constraint failed: model_platforms.api_key
```

**根因**

[数据库定义](D:/Agent/Project/OMNIX-Workbench/src-tauri/src/db_schema.rs:552)将 `api_key` 声明为 `TEXT NOT NULL`，没有默认值。旧版 `save_model_platform` 的 INSERT 列表省略了该列。SQLite 因此尝试填入 NULL；即使用户在表单里填写了 Key，也不会改变这个 INSERT 的行为。UPSERT 的候选插入行同样需要满足这个约束，所以已有平台的保存/开关操作也会受影响。

**已实施**

[保存实现](D:/Agent/Project/OMNIX-Workbench/src-tauri/src/commands/platforms.rs:360)在 INSERT 中明确写入空字符串作为旧列兼容值；ON CONFLICT 更新不覆盖已有 Key。首次配置 Key 时写入加密表，并在同一事务内向旧列写入相同密文，以兼容现有旧读取入口；两处都不写明文。无 Key 的本地平台保持空字符串。

不需要重建数据库、清空数据或要求用户删除旧配置。

**验证**

新增测试在真实初始化的临时 SQLite 库中执行旧 INSERT，准确复现同一报错，再验证新保存逻辑可用。覆盖有 Key 云平台与无 Key 的 Ollama。

### R02 · P1：平台和初始 Key 分开保存，后一步失败会被隐藏

**根因**

原 [usePlatforms](D:/Agent/Project/OMNIX-Workbench/src/hooks/usePlatforms.ts:153)先保存平台，再调用另一个 IPC 保存 Key；第二步失败只写 `console.error`，随后关闭表单。用户可能看到保存成功，却得到一条没有凭据的平台配置。

**已实施**

后端使用 SQLite `IMMEDIATE` 事务统一保存平台和首个加密 Key。任一步失败都会返回错误并回滚。前端移除第二次独立写入，失败时保留表单输入。

[平台弹窗](D:/Agent/Project/OMNIX-Workbench/src/components/modals/PlatformModal.tsx)增加保存中状态和同步防重复提交锁，保存期间避免关闭弹窗后又触发重复请求。

**验证**

通过临时数据库触发器分别强制让 Key INSERT、旧列兼容写入失败，确认平台与 Key 记录整体回滚、错误被返回；解除故障后可重新保存。重复保存不会重复添加首个 Key，且新表与旧列保存的是相同密文。

### R03 · P1：启动迁移后，发现模型与批量检测可能读不到 Key

**根因**

启动迁移清空 `model_platforms.api_key`；旧实现的 `fetch_remote_models`、`batch_check_models` 仍然直接查询该列，而路由和单模型检测已经使用 `platform_keys`。

这使同一个平台可能出现“单模型测试有 Key，批量测试却说没有 Key”的矛盾结果。

**已实施**

两个入口通过 [platform_connection_config](D:/Agent/Project/OMNIX-Workbench/src-tauri/src/commands/platforms.rs:2111)读取平台配置，并用 `platform_keys` 解析当前密钥。保留旧数据的回退读取能力。本项只覆盖这两个入口，不代表所有旧读取入口都已迁移；剩余问题见 R11。

**验证**

临时库模拟两把 Key 和活跃 Key 切换，再执行真实启动迁移，确认旧列清空后仍读取所选 Key。此测试验证数据库/配置读取链路，不声称已测试外部供应商的 HTTP 服务。

### R04 · P2：编辑已禁用平台会将其重新启用

**根因**

前端构造保存对象时固定设置 `is_enabled: true`。修改平台名称或地址也会改变其启用状态。

**已实施**

编辑时保留 `editingPlatform.is_enabled`，只有新建平台默认启用。现有路由权重、优先级、密钥和子模型不因普通编辑被重置。

**验证**

Rust 回归测试覆盖已有平台修改、禁用、保留 Key/路由/模型；前端通过类型检查和现有测试。新增界面交互测试仍是后续建议。

## 4. 待修复的问题

### R05 · P1：旧密钥迁移写入失败后，仍会清空原值

**证据**

[migrate_legacy_plaintext_keys](D:/Agent/Project/OMNIX-Workbench/src-tauri/src/commands/platforms.rs:524)以毫秒时间戳和下标组成新 Key ID；INSERT 结果只用 `.is_ok()` 计数。随后无条件清空旧列，不将插入失败向上传递。

因此，只要 Key 写入失败而后续 UPDATE 成功，原始凭据就可能丢失。跨平台同毫秒、相同下标生成重复 ID 也是一个可发生的失败条件。启动层虽然有迁移告警，但收不到这里被吞掉的错误。

**修改建议**

- 每个平台的读取、加密写入、完整性验证和旧列清理放进同一事务。
- 仅在所有凭据成功迁移并校验后清空旧值；失败回滚并返回错误。
- 使用包含平台身份的稳定 ID 或可靠的随机 ID，并约束重复迁移。
- 迁移提示区分“未完成”和“没有需要迁移的项”。

**验收**

故障注入覆盖 INSERT 失败、ID 冲突、部分迁移后重试；失败前后原有凭据可恢复，不能只断言迁移计数。

**状态**：静态确认，待修复；本次未在用户数据库上注入故障。

### R06 · P1：回退前自动备份失败，仍继续覆盖工作区

**证据**

[restore_checkpoint](D:/Agent/Project/OMNIX-Workbench/src-tauri/src/commands/checkpoints.rs:304)用 `let _ = create_checkpoint_core(...)` 丢弃备份错误，随后执行文件覆盖和删除。

[界面确认文案](D:/Agent/Project/OMNIX-Workbench/src/components/WorkspaceCheckpoints.tsx)明确承诺回退前会自动创建备份点。当前行为与承诺不一致。该恢复函数还忽略了部分文件枚举/删除错误，可能在恢复不完整时返回成功。

**修改建议**

- 将回退前备份成功作为恢复前置条件，失败即停止并说明原因。
- 验证目标检查点和预备份引用后再开始覆盖。
- 文件删除与枚举错误必须收集并报告；不能返回无条件成功。
- 记录恢复事务的阶段，便于中断后诊断和重试。

**验收**

模拟 Git 快照失败和被锁定文件：恢复应停止或明确报告部分失败，不能丢失“回退前”的用户改动。

**状态**：静态确认，待修复；本次未对真实项目执行恢复操作。

### R07 · P2：删除模型平台后，加密 Key 记录仍会残留

**证据**

[delete_model_platform](D:/Agent/Project/OMNIX-Workbench/src-tauri/src/commands/platforms.rs:553)只删除平台。[`platform_api_keys` 建表语句](D:/Agent/Project/OMNIX-Workbench/src-tauri/src/db_schema.rs:974)没有平台外键；检索未发现对应级联清理。相较之下，`platform_models` 明确设有 `ON DELETE CASCADE`。

**影响**

平台消失后，相应加密凭据仍留在数据库，产生孤立记录，也不符合用户删除连接的直觉。

**修改建议**

把子 Key 清理和平台删除放进同一事务；规划外键迁移，并在迁移前列出已有孤立记录的处理办法。不要仅在新建库上补外键而忽略升级库。

**验收**

创建含两把 Key 的平台后删除，平台、Key、子模型均按约定清理；模拟删除失败时保持一致。

**状态**：静态确认，待修复。

### R08 · P2：非 ASCII Key 可使密钥掩码代码 panic

**证据**

[mask_api_key](D:/Agent/Project/OMNIX-Workbench/src-tauri/src/commands/platform_api_keys.rs:34)直接使用 `&key[..4]`、`&key[key.len() - 4..]` 按字节截取 Rust 字符串。

例如错误粘贴“密钥测试字符串”时，字节 4 不在 UTF-8 字符边界，触发 panic。常见厂商 Key 多为 ASCII，但输入框未限制该条件，错误输入应可正常报错。

**修改建议**

复用 [crypto::mask_secret](D:/Agent/Project/OMNIX-Workbench/src-tauri/src/crypto.rs)的按字符处理实现；如协议只允许 ASCII，也应返回可理解的校验错误。

**验收**

覆盖空串、短 Key、正常 ASCII、中文和 emoji，任何输入都不得触发 panic；短 Key 不应泄露完整内容。

**状态**：静态确认，待修复。

### R09 · P2：切换任务/工作区后，预览可能仍显示上一个工作区内容

**证据**

[AppStoreProvider](D:/Agent/Project/OMNIX-Workbench/src/store/AppStore.tsx:105)在同一个长期存在的 Provider 内调用 `usePreview(convs.chatWorkspace)`。[usePreview](D:/Agent/Project/OMNIX-Workbench/src/hooks/usePreview.ts:29)使用一组全局状态，没有在工作区变化时重置已选文件/内容，也没有请求序号保护。

**影响**

从工作区 A 切换到 B 后可能继续显示 A 的预览；快速选择文件时，较晚返回的旧请求也可能覆盖新选择。这会影响对 Agent 改动的判断，是任务桌面需要优先处理的状态归属问题。

**修改建议**

以会话/工作区 ID 管理预览状态。切换时清空或加载对应快照；所有异步读取携带上下文 ID 与请求序号，过期结果不更新当前面板。

**验收**

A→B 切换后不能显示 A 内容；让 A 请求晚于 B 返回，确认 B 面板不被覆盖；空工作区不保留旧文件列表。

**状态**：静态确认，待修复；未进行原生 UI 竞态复现。

### R10 · P2：模型列表异步返回未校验当前平台

**证据**

[selectPlatform](D:/Agent/Project/OMNIX-Workbench/src/hooks/usePlatforms.ts:132)先设置所选 ID，再无条件将异步结果写入 `platformModels`。`fetchRemoteModels` 和 `batchTestModels` 也直接替换当前列表。

**影响**

快速切换 A→B 时，A 的慢请求可能在 B 选中后回写。界面标题指向 B、模型行却属于 A，后续检测/删除容易被误操作。列表加载失败也只写 console，没有清晰的可见错误状态。

**修改建议**

按平台维护列表缓存和请求状态；响应提交前检查平台 ID/请求序号；切换期间清空旧列表或明确标记加载中；失败提供可见错误与重试入口。

**验收**

使用可控制返回顺序的 Promise 测试 A/B 切换、批量检测期间切换、请求失败后的列表归属。

**状态**：静态确认，待修复。

### R11 · P1：其他旧认证入口仍可能使用空 Key 或密文本身

**证据**

- [CLI 接管的平台凭据解析](D:/Agent/Project/OMNIX-Workbench/src-tauri/src/commands/cli_takeover.rs:129)仅查询旧列并解密；启动迁移清空旧列后，会解析为空 token。
- [上游模型同步](D:/Agent/Project/OMNIX-Workbench/src-tauri/src/commands/lifecycle.rs:646)和[批量平台健康检查](D:/Agent/Project/OMNIX-Workbench/src-tauri/src/commands/lifecycle.rs:808)仍查询旧列，且向 HTTP Authorization 传值前没有解密。旧列有密文时会发送密文；被迁移清空后又变成无凭据请求。
- [知识库向量模型解析](D:/Agent/Project/OMNIX-Workbench/src-tauri/src/knowledge.rs:342)的指定平台分支虽查询新表，却把 `encrypted_key` 原值返回；自动选择分支仍只读旧列。后续 `generate_embeddings` 直接将该值用于 Authorization。

**影响**

可能出现“模型中心检测成功，但同步、CLI 接管或知识库调用失败”。健康检查还可能把认证读取错误误归因为平台不健康，影响后续路由选择。

**修改建议**

统一使用返回已解密凭据的解析接口；明确活跃/禁用 Key、旧数据回退和缺少凭据的行为。所有读取方迁移完成后，再移除密文兼容镜像写入，不能只调整存储侧。

**验收**

用本地 HTTP mock 检查实际收到的认证头：首次保存、切换 Key、应用重启迁移、禁用 Key 后，各入口都应符合相同规则；认证头不得是 `ENC:v2:` 密文。测试不得调用真实付费接口。

**状态**：静态确认，待修复。此次仅保留首次保存的密文兼容行为，未改造这些入口。

## 5. 任务桌面的产品改造建议

以下是产品建设建议，不是声称已经实现的功能。

### D01 · 先完成任务级面板归属与恢复

[现有预览](D:/Agent/Project/OMNIX-Workbench/src/components/layout/PreviewPane.tsx)是固定侧栏；[工作会话](D:/Agent/Project/OMNIX-Workbench/src/components/tabs/ChatTab.tsx)已经包含计划与检查点面板。先以这些真实功能做桌面适配，建立统一的面板 ID、任务 ID、资源引用、布局版本和恢复状态。

第一批只接文件、Diff、运行日志、预览、测试报告。重启后恢复面板描述和资源定位；对于已经退出的终端/进程，显示“已结束”，不能把恢复布局误当成恢复活进程，更不能自动重跑命令。

### D02 · 交付物与验证证据分开建模

建议复用现有会话、run 和步骤标识；不要平行创建另一套互不关联的任务系统。

交付物记录路径/版本/哈希、来源 Agent 与步骤；验证单独记录检查对象版本、命令、退出码和报告。产物发生变化时，旧验证自动失效。用户的接受/拒绝也应独立记录，避免“测试通过”自动等于“用户接受”。

### D03 · 默认入口提供“继续最近工作”

[当前入口](D:/Agent/Project/OMNIX-Workbench/src/App.tsx:196)默认是 `chat`；[导航注册表](D:/Agent/Project/OMNIX-Workbench/src/lib/appRegistry.tsx)已经将对话、工作、团队与低频应用分开。保留这套基础，在“工作”首屏加入恢复最近任务的入口，避免先要求用户布置一整张空桌面。

### D04 · Agent 打开面板不能抢占用户焦点

Agent 可申请打开或更新与任务有关的面板，由宿主去重和定位。默认更新后台面板或交付架提示；需要用户决策时再聚焦。关闭面板不等于终止后台任务，两者要有独立操作。

## 6. 建议实施顺序与验收

| 阶段 | 修改项 | 完成依据 |
|---|---|---|
| 本次修复 | R01–R04 | 已完成源码修复、回归测试与构建检查 |
| 下一批：数据可靠性 | R05、R06 | 故障注入证明迁移不丢 Key、备份失败不继续覆盖 |
| 下一批：认证一致性 | R11 | 本地 HTTP mock 与 CLI 配置解析确认各入口正确读取活跃 Key |
| 下一批：配置与状态 | R07–R10 | 删除清理、Unicode 输入、异步乱序与上下文切换测试通过 |
| 任务桌面第一阶段 | D01、D03 | 能恢复同一任务的文件、Diff、日志、预览；关闭/重启后归属正确 |
| 任务桌面第二阶段 | D02、D04 | 交付物可追溯、验证绑定内容版本、用户可审核且焦点稳定 |

优先把“配置能保存、错误能看见、数据不丢、任务现场归属正确”做好，再扩展窗口自由布局和第三方面板。

## 7. 本次代码修改与使用说明

修改文件：

- [模型平台保存和读取](D:/Agent/Project/OMNIX-Workbench/src-tauri/src/commands/platforms.rs)
- [模型保存回归测试](D:/Agent/Project/OMNIX-Workbench/src-tauri/src/commands/platforms/save_tests.rs)
- [平台表单状态](D:/Agent/Project/OMNIX-Workbench/src/hooks/usePlatforms.ts)
- [保存中和防重复提交](D:/Agent/Project/OMNIX-Workbench/src/components/modals/PlatformModal.tsx)
- [测试密钥隔离](D:/Agent/Project/OMNIX-Workbench/src-tauri/src/crypto.rs)

加密模块的新增隔离仅作用于 `cfg(test)`：测试生成进程内随机密钥，不使用用户的真实加密密钥文件；生产加密行为保持现有实现。

本次没有修改数据库结构，也没有删除或重置用户已有模型配置。修复需要运行新构建的程序；只重启旧安装版不会让源码修复生效。

### Windows 修复版

- 文件：[omnix-workbench.exe](D:/Agent/Project/OMNIX-Workbench/src-tauri/target/release/omnix-workbench.exe)
- 生成时间：2026-09-10 20:53:26（Asia/Shanghai）
- 文件大小：39,235,072 字节
- SHA-256：`45784F8E735F8D20759D9E818E091ED009E07E6EB68218AD17944A38D7309BF7`
- 使用方式：先退出旧程序，再运行上述 EXE，然后重新尝试添加平台。这是本地修复构建，版本号仍为 `0.36.0`，不是已发布的新版本或安装包。
- 保留现有用户配置；不要为解决此次 NOT NULL 错误删除数据库或加密密钥文件。
