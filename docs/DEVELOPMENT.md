# Veya 开发规范

本文面向贡献者和 Agent，记录当前 workspace 的可复现开发路径。产品范围、已实现能力和用户侧限制见 [README.md](../README.md)；模块职责和数据语义见 [ARCHITECTURE.md](ARCHITECTURE.md)。

## 环境

Veya 是 Windows-only 的 Rust workspace，当前使用 Rust 2021 和 MSVC 工具链。准备以下环境：

- Windows 10/11 开发机或等价的 Windows CI runner；
- Rust stable 的 `x86_64-pc-windows-msvc` 工具链；
- Visual Studio Build Tools 的 MSVC 编译工具和 Windows SDK；
- PowerShell 和 Cargo。

检查工具链和编译器：

```powershell
rustc -Vv
cargo -V
rustup show active-toolchain
```

仓库使用 bundled SQLite，不需要单独安装 SQLite。Windows 资源编译由桌面 crate 的构建脚本完成。

## 常用命令

在仓库根目录执行：

```powershell
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
cargo build -p veya-desktop
```

`cargo test --workspace` 是默认行为门禁，覆盖 core 的流程语义、storage 的持久化测试和桌面端的纯逻辑测试；Windows 平台模块的真实消息循环、托盘、剪贴板和窗口行为仍需按下节手动验收。只通过 `cargo check` 不能证明这些交互正确。

仓库的 `.github/workflows/ci.yml` 在 `windows-2022` 上安装 stable MSVC 工具链和 `rustfmt`，依次执行格式检查、workspace 测试、workspace 检查和 release 编译，并检查产物使用 Windows GUI 子系统，避免启动时出现控制台窗口。测试步骤把 `APPDATA` 指向 GitHub Actions 的临时目录；workflow 不依赖开发机缓存、用户数据库或真实剪贴板。

本地安装包使用 NSIS。安装 NSIS 后执行 `./scripts/build-installer.ps1`，产物位于 `artifacts/veya-setup.exe`。组件页默认勾选开始菜单和桌面快捷方式；快捷方式使用随安装包部署、以图标内容摘要命名的独立 ICO，避免同一路径下 Explorer 沿用旧图标缓存。安装完成页提供启动 Veya 的选项；卸载会移除快捷方式、图标和程序文件。安装器就地覆盖旧版 `veya.exe`，保留用户历史和设置；卸载也不清理 `%APPDATA%\Veya`。实际安装、覆盖升级和卸载仍要用隔离数据目录及 Windows 虚拟机或测试机手动验收。

## 隔离 Windows 运行数据

运行桌面端会在 `%APPDATA%\Veya\veya.db` 使用本地 SQLite 数据库。手动验收时，为每次开发运行创建独立的 `APPDATA`，避免读写日常数据：

```powershell
$oldAppData = $env:APPDATA
$isolatedAppData = Join-Path $env:TEMP ("Veya-dev-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $isolatedAppData | Out-Null
$env:APPDATA = $isolatedAppData
try {
    cargo run -p veya-desktop
}
finally {
    $env:APPDATA = $oldAppData
}
```

停止程序后，可在 `$isolatedAppData\Veya\veya.db` 检查本次运行的数据。这个环境变量只隔离 Veya 的数据库；Windows 系统剪贴板仍是真实系统资源，手动操作使用专门的测试文本和测试应用。确认程序退出后再继续其他剪贴板工作。

## Windows 手动验收

涉及 `veya-windows`、`worker`、Iced 交互或系统动作时，在隔离 `APPDATA` 的运行实例上逐项记录结果：

1. 窗口默认以 `1080×700` 启动，在鼠标所在显示器的工作区居中，且不额外打开终端窗口；工作区较小时窗口缩至可见范围。分别检查主屏、副屏及不同缩放比例，并确认标题、圆角、边框、字体和托盘入口可用。用左上角品牌区和顶部“拖动窗口”区域拖动窗口，确认搜索框和按钮仍可正常操作。检查正常追踪状态下托盘图标在 100%/150%/200% 缩放时的视觉大小，以及安装后的桌面快捷方式图标在常见缩放比例下的大小，不应因源图透明留白显著小于同类应用图标；
2. 第二次启动只激活已有窗口，不创建第二个实例；
3. 在已知测试应用复制一段安全文本（可包含 `❤`、`😀` 等字符），历史列表、详情预览和完整内容弹窗正确显示原文及可获得的来源信息；
4. 在另一个测试应用触发 `Ctrl+V`，详情中的使用记录反映观察到的目标；同时把它理解为快捷键观察结果，不把它当作插入成功证明；
5. 搜索、内容分类、最新/最旧排序、紧凑/详细列表切换、记录选择和滚动保持可用；两种列表密度下右键任意卡片，确认菜单在鼠标位置弹出且不超出窗口，卡片被选中，固定/取消固定与删除只作用于该卡片，Esc 或点击菜单外可收起；
6. 详情中的复制、复制为纯文本、打开链接/来源、删除和完整内容弹窗保持可用，弹窗可关闭；
7. 设置中的记录开关、保留期、排除应用和清空历史反馈正确。反复点击追踪开关和顶部暂停按钮，确认状态不会短暂跳回；排除应用时用与实际进程名大小写不同的文件名（如 `chatgpt.exe` 对 `ChatGPT.exe`），确认后续复制不新增记录、旧记录仍保留；取消排除后再次复制应新增记录。重启隔离实例确认排除设置持久化；
8. 改动涉及托盘、单例、窗口样式或 shell 动作时，额外检查对应真实 Windows 行为和失败反馈。
9. 检查 `Alt+V` 在 Veya 窗口可见时隐藏，即使焦点在其他应用；最小化和托盘隐藏时应唤起窗口。托盘“打开窗口”和重复启动始终唤起。录制自定义组合后确认新组合生效、旧组合释放，重启后仍生效；关闭组合后确认没有全局窗口快捷键。托盘不可用时，可见窗口按快捷键应最小化而不失去恢复入口。
10. 先把焦点放在“排除应用”输入框，再点击“更改”录制快捷键，确认按键没有写入该输入框；按 Esc 或切到其他窗口后，旧组合应恢复。组合被其他程序占用时，设置页应显示冲突，原组合继续可用。
11. 从 Explorer 复制单个文件、多个文件和文件夹，确认文件筛选、路径详情和重启恢复；在 Veya 中重新复制后用 Explorer 粘贴，确认仍是文件列表且没有新增 Veya 自己的历史。移动或删除源路径后重新复制应提示失败且不清空原剪贴板。
12. 从截图工具及浏览器复制图片，确认图片筛选、实际缩略图、尺寸和重启恢复；点击缩略图和“查看图片”打开弹窗，确认完整图片可缩放查看并关闭；在 Veya 中重新复制后用支持图片的应用粘贴，确认没有新增 Veya 自己的历史。观察文本/图片并存的剪贴板变更只生成一条记录；不支持或超限的格式只显示跳过状态。
13. 固定文本、文件和图片记录，组合分类与“已固定”筛选，重启后确认状态仍在；自动保留期只清理未固定记录，取消固定后重新适用。手动删除与确认后的“清空全部历史”都可删除固定记录。
14. 在设置页点击“检查更新”，确认当前版本、无发布版本或新版本的提示；“打开发布页”和“访问仓库”应交给系统浏览器。发现新版本且 GitHub 资产提供 SHA-256 时，点击“下载并安装”后应先校验大小和摘要，再请求 Windows 启动安装器；校验失败不能运行安装器。检查更新本身不自动下载或执行安装包。
15. 从提权的安装器完成页启动 Veya，确认最终运行进程与 Explorer 处于同一普通权限级别；让 Veya 窗口保持前台，用普通权限截图工具的 F3 快捷键截图。分别检查直接从桌面快捷方式启动和安装器完成页启动。

截图或手动验收报告应注明 Windows 版本、构建命令、隔离数据目录和未验证的项目。只报告实际操作过的行为。

## 按改动选择验证

| 改动 | 最低验证 |
| --- | --- |
| `veya-core` 事件、FlowEngine、搜索或聚合 | `cargo test -p veya-core`，必要时补充行为测试 |
| `veya-storage` schema、迁移或读写 | `cargo test -p veya-storage`，使用临时数据库，不使用 `%APPDATA%\Veya` |
| `veya-windows` 平台、托盘、单例、图标或 shell | `cargo check --workspace`，再做隔离运行的真实 Windows 验收 |
| `veya-desktop` 消息、历史、详情、设置或主题 | `cargo test -p veya-desktop`（适用时），再做真实窗口验收 |
| 跨 crate 或公共接口 | `cargo fmt --all -- --check`, `cargo check --workspace`, `cargo test --workspace` |
| 文档、CI 或开发流程 | 检查命令、路径和描述与当前代码/配置一致 |

新增测试应验证行为边界或回归风险。不要用测试替代真实 Win32 交互，也不要把用户数据库或真实剪贴板作为测试夹具。

## 改动范围与提交

编辑前后都检查 `git status --short` 和目标文件的 diff。按职责拆分改动：core 语义、storage 持久化、Windows 适配、worker 编排和 Iced 视图分别保持清晰；不要顺手格式化或重写无关的已有脏改动。

提交前让提交内容只包含当前目标所需文件，提交说明写清用户可见行为或模块边界，并列出实际运行过的验证命令。推送到 `NGLSL/Veya` 前须核对 remote 和目标分支。

## 发布流程

1. 更新 workspace 版本，在 `docs/releases/vX.Y.Z.md` 写人工发布说明，覆盖变化、升级影响、验证和已知限制。
2. 将候选提交推到 GitHub 默认分支，等待该提交的 `CI` push run 成功；确认 Windows 安装、覆盖升级及卸载的实际验收结果。
3. 在同一提交创建并推送注释标签 `vX.Y.Z`。`release.yml` 检查标签格式、Cargo 版本、发布说明、默认分支包含关系和该提交的成功 CI 后，重新运行测试、构建 NSIS 安装包、生成 SHA-256 并发布 GitHub Release。
4. 发布后下载安装包，独立复算 SHA-256，并核对发布页和安装行为。仓库首次推送与实际发布属于单独操作，配置文件本身不会触发发布。

如果标签已推送而 Release workflow 在创建 Release 前失败，先修复工作流并提交到默认分支，再在 GitHub Actions 手动运行 `Release`，传入原有的 `vX.Y.Z` 标签。也可执行 `gh workflow run release.yml --repo NGLSL/Veya --ref main -f tag=vX.Y.Z`。重试仍检验该注释标签的版本、默认分支包含关系和标签提交的成功 CI；不要移动已发布的标签。

## Issue 与文档

计划和实现记录使用 `.scratch/<feature>/spec.md` 与 `issues/NN-*.md`，规则见 [issue-tracker.md](agents/issue-tracker.md)。涉及术语或职责变化时先核对 [domain.md](agents/domain.md) 和 [ARCHITECTURE.md](ARCHITECTURE.md)；文件、图片和固定能力的支持范围与未验收场景以 [README.md](../README.md) 和对应 issue 为准。
