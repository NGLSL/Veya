# Veya 全局快捷键切换窗口

Status: ready-for-human
Feature: veya-window-hotkey

## 目标

用户在 Veya 正常运行时，通过全局快捷键切换主窗口的显示状态；托盘“打开窗口”和重复启动始终唤起。唤起后窗口可见、位于前台、落在可用显示器工作区内，原有单例与剪贴板跟踪继续工作。

## 规划时基线（2026-09-23）

- `veya-desktop/src/main.rs` 已有单例守卫和启动时的鼠标所在显示器定位；重复启动通过 `veya-windows/src/platform/singleton.rs` 的激活事件恢复最小化窗口。
- `veya-desktop/src/app.rs` 的最小化按钮保留窗口；关闭按钮发送停止跟踪并关闭 Iced 主窗口。托盘 `OpenWindow` 命令目前是空分支，托盘“设置”只切换页面。直接为快捷键增加热键监听，无法解决关闭后无窗可唤起的问题。
- `veya-windows/src/platform/win.rs` 已有隐藏消息窗口和消息泵；`keyboard.rs` 的低级键盘钩子用于观察粘贴快捷键，不能把唤起当成粘贴事件。
- 本机 Kite 当前默认使用 `Alt+Space`。按用户 2026-09-23 的选择，Veya 默认使用 `Alt+V`，设置页允许更换或关闭。

## 行为约定

- 按用户后续反馈，全局快捷键按可见状态切换：窗口可见且未最小化时按键隐藏到托盘，即使焦点在其他应用；最小化或已隐藏时按键显示并聚焦。托盘不可用时改为最小化，保留任务栏恢复入口。托盘“打开窗口”、重复启动和“打开设置”始终唤起，不执行切换。
- 关闭主窗口后进程留在托盘、剪贴板跟踪保持原设置；托盘“退出”才结束进程并释放热键。若托盘创建失败，应有明确的关闭/退出退路，不能留下不可见且不可恢复的进程。
- 窗口从隐藏/最小化恢复时，按现有多显示器和 DPI 规则确保仍在可见工作区；已显示窗口保留用户当前位置。当前焦点应用不应被重复按键导致粘贴触发或剪贴板记录。
- 热键冲突、无效设置或注册失败须反馈，不能在界面宣称已生效；跟踪暂停与热键唤起互不影响。进程退出或更换组合时清理旧注册。

## 技术边界

- Windows 系统热键归 `veya-windows`：优先复用已有隐藏消息窗口，在它所属线程调用 `RegisterHotKey`，处理 `WM_HOTKEY`，结束时 `UnregisterHotKey`。使用 `MOD_NOREPEAT` 避免长按反复唤起。[Microsoft RegisterHotKey](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-registerhotkey)、[UnregisterHotKey](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-unregisterhotkey)。
- Iced 窗口显示/聚焦由桌面层统一协调；Windows 层在热键消息到达时采样窗口可见性和最小化状态并传递切换信号，桌面层决定隐藏或唤起。托盘和单例通知保持单向唤起；低级键盘钩子继续只负责粘贴触发。
- 默认组合为 `Alt+V`。它不在 [Microsoft 列出的 Windows 系统快捷键](https://learn.microsoft.com/en-us/windows/win32/menurc/about-keyboard-accelerators)中；个别应用或网页会把它作为自己的菜单访问键，例如 [SharePoint Server 2010 旧界面的 View 菜单](https://support.microsoft.com/en-us/sharepoint/sites-pages/keyboard-shortcuts)。因此要在常用目标应用中核对全局热键对原有按键的影响，保留改键入口。若其他进程已注册同一全局组合，保留可用的旧绑定或清楚报告当前无绑定。不要静默覆盖 Kite 的 `Alt+Space`。

## 交付顺序

1. [01 窗口生命周期与统一唤起](issues/01-window-activation.md)：先修托盘打开窗口的空动作、关闭到托盘与退出，复用单例恢复路径。
2. [02 全局热键注册](issues/02-global-hotkey.md)：在 Windows 消息线程注册和清理 `Alt+V`，接到统一唤起。
3. [03 设置与 Windows 验收](issues/03-settings-and-validation.md)：可见的组合/冲突状态、持久化及真实窗口交互验证。

## 验收边界

- 真机覆盖：前台、后台、最小化、关闭到托盘、多个显示器/DPI、与 Kite 同时运行、热键冲突、常用应用的 `Alt+V` 菜单行为、重复启动、退出后热键释放；托盘打开/设置/退出都能完成对应动作。
- 自动测试覆盖热键解析/配置和窗口命令路由中可纯测的部分。`cargo test` 或编译通过不能替代 Windows 键盘、托盘和焦点验证。
- 不增加剪贴板内容类型，不改变 Ctrl+V/Shift+Insert 的“观察到粘贴触发”含义；本轮不做热键唤起性能承诺。

## 实现与待验收（2026-09-23）

- 已实现窗口恢复、可见窗口热键隐藏、关闭到托盘、托盘打开/设置/退出、单例通知、Win32 全局热键注册和释放，以及设置页录制、默认/关闭、冲突反馈和 SQLite 持久化。录制期间排除应用输入框不渲染；取消或窗口失焦时恢复原组合。
- `cargo test --workspace`、`cargo check --workspace`、`cargo build -p veya-desktop` 已通过。隔离 `APPDATA` 的 Windows 进程中，Alt+V 注册占用可观测；把数据库设为 `Ctrl+Alt+K` 后重启，新组合占用而 Alt+V 释放；设为 `disabled` 后重启两者均释放。第二次启动退出且把已最小化的原窗口恢复，进程仍只有一个。
- 后续在解锁桌面复现了“窗口可见但焦点在其他应用”时旧版 Alt+V 只聚焦、不隐藏；前台再按一次能隐藏，说明隐藏任务有效，切换条件需要改为可见状态。修正后的代码和测试已通过，真实按键回归仍需在退出正在运行的旧发布版后测试。设置页录制焦点、托盘退出、注册冲突提示、Kite 共存以及多显示器/DPI 仍按 `docs/DEVELOPMENT.md` 验收。
