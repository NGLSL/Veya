# Veya

Veya 是一个 Windows 剪贴板流转记录器：记录文本、现有文件路径列表和图片从哪里复制，以及观察到它们被粘贴到哪里。

当前项目是 Rust/Iced 桌面应用源码，版本号由 `Cargo.toml` 管理。它面向本机使用，数据写入本地 SQLite。仓库已配置 Windows CI 和安装包发布流程；可以从源码构建，已发布版本的安装包可从 GitHub Releases 获取。

## 当前能力

- 捕获 Windows 剪贴板中的 Unicode 文本、`CF_HDROP` 本地文件或文件夹列表，以及 `CF_DIB`/`CF_DIBV5` 图片；记录来源进程、窗口和来源置信度。同一次变更只保存优先匹配的一种载荷。
- 观察 `Ctrl+V` 和 `Shift+Insert` 粘贴快捷键，把目标进程、窗口和时间显示在记录的使用时间线中。
- 将原始复制事件保存到本地 SQLite；历史列表可以把短时间内相同内容的复制事件聚合成一张展示卡片，原始事件仍然分别保存。
- 在历史记录中搜索内容、来源应用和观察到的目标应用；按普通文本、链接、代码、文件或图片分类查看。“已固定”可与内容分类、搜索和排序组合；列表可按最新或最旧排序，并切换详细/紧凑布局。
- 查看记录详情和图片预览，把文本、文件列表或图片按原载荷重新复制到系统剪贴板；文本还可复制为纯文本、打开链接、使用浏览器搜索，或在弹窗中查看全文。
- 暂停或恢复追踪，设置 `1 天`、`7 天`、`30 天` 或 `永不` 的保留期，排除指定应用的后续复制，固定/取消固定记录，删除记录或清空全部历史。右键历史卡片会在鼠标位置弹出固定和删除菜单。固定记录跳过自动到期清理，直到取消固定或手动删除；加入排除名单不会自动删除已有记录。
- 使用 Windows 托盘菜单暂停追踪、打开窗口或设置、清空历史和退出程序；托盘可用时，关闭主窗口会保留后台追踪进程。
- 默认按 `Alt+V` 切换窗口：窗口可见时隐藏，已隐藏或最小化时唤起。设置页可录制自己的 `Ctrl` 或 `Alt` 组合键，也可恢复默认或关闭快捷键。组合键保存在本地设置中；冲突或注册失败会在设置页提示。重复启动始终激活已有实例。
- 设置页可手动检查 [GitHub Releases](https://github.com/NGLSL/Veya/releases) 的最新版本，打开发布页或 [Veya 仓库](https://github.com/NGLSL/Veya)。检查更新会向 GitHub API 发送请求；发现新版本后，只有用户点击“下载并安装”才会下载并校验安装包。发布资产缺少可验证的 SHA-256 时，可到发布页手动下载。

## 当前边界

- 文件记录保存的是本地文件或文件夹的路径列表，不保存文件本身或剪切意图；重新复制前会检查路径是否仍存在。虚拟文件和其他 OLE 专有对象尚不支持。
- 图片捕获覆盖 `CF_DIBV5` 和 `CF_DIB`，保存为 PNG 并以 `CF_DIBV5` 重新复制；其他图片格式、损坏或超限数据会跳过。具体应用能否提供这些格式取决于它的剪贴板实现。
- 来源应用可能只能通过前台窗口推断，界面会标注这种不确定性。
- 使用记录表示检测到了粘贴快捷键和前台目标，不能证明目标应用已经成功插入内容。
- 链接和代码分类是针对文本内容的启发式展示分类；看起来像文件路径的纯文本仍归为文本。
- “复制来源”与“使用记录”来自本机 Win32 观察结果；应用切换、权限或系统限制可能导致来源或目标为空。

## Windows 构建要求

需要一台 Windows 机器，以及：

- Rust stable 工具链（`rustup`）；
- MSVC Rust 工具链和对应的 Visual Studio Build Tools；
- Windows SDK。

项目使用 `winres` 嵌入 Windows 图标，MSVC 工具链和 Windows SDK 是桌面端构建所需的组成部分。

## 安装包与发布

发布后从 [GitHub Releases](https://github.com/NGLSL/Veya/releases) 下载 `veya-setup.exe`，并核对同页的 `veya-setup.exe.sha256`。安装器需要管理员权限写入 Program Files；覆盖安装会保留 `%APPDATA%\Veya` 中的历史和设置，卸载也不会删除这些用户数据。安装包目前未签名，Windows 可能显示未知发布者。

本地构建安装包需要 NSIS：

```powershell
./scripts/build-installer.ps1
```

发布门禁和步骤见 [开发规范](docs/DEVELOPMENT.md)。配置发布 workflow 不等于已经发布；只有向 GitHub 推送符合条件的版本标签后才会创建 Release。

## 从源码构建和运行

在仓库根目录执行：

```powershell
cargo test --workspace
cargo check --workspace
cargo build -p veya-desktop
cargo run -p veya-desktop
```

运行 `cargo run -p veya-desktop` 会启动 Veya 桌面程序。Windows 版本的窗口、剪贴板监听、键盘钩子和托盘行为需要在 Windows 实机上验证；非 Windows 环境不会提供这些平台能力。

提交前还应检查格式：

```powershell
cargo fmt --all -- --check
```

## 本地数据与隐私

默认数据库路径为：

```text
%APPDATA%\Veya\veya.db
```

剪贴板文本、文件路径列表、图片像素、来源和观察到的粘贴记录保存在这台机器的 SQLite 数据库中。Veya 的核心记录流程和数据库层没有云端同步接口；只有当用户主动使用“打开链接”或“搜索”等操作时，内容才会交给对应的外部应用或浏览器。

设置中的保留期会定期删除过期且未固定的记录；“清空全部历史”会删除包括固定记录在内的本地记录及其粘贴触发记录。需要保留数据时，请在操作前自行备份数据库文件。

## 项目结构与开发文档

- [架构与数据流](docs/ARCHITECTURE.md)：四个 crate 的职责、事件流、数据语义和跨层边界。
- [开发规范](docs/DEVELOPMENT.md)：Windows/MSVC 环境、验证命令、隔离数据目录和不同改动的验收方式。
- [Agent 协作入口](AGENTS.md)：目录职责、领域不变量和文档导航。
- [本地 issue 规则](docs/agents/issue-tracker.md)：`.scratch/` 下规格和实现 issue 的格式。
- [当前工程基础规格](.scratch/veya-project-foundation/spec.md)：文档、职责拆分和验证门禁的实施顺序。

修改剪贴板语义、存储 schema、Win32 行为或 Iced 交互时，请先阅读架构文档，再按开发规范运行与改动范围相匹配的检查。
