# 05：收拢桌面端系统动作

Status: done
Blocked by: 04

## 要做什么

检查 `app.rs` 中窗口圆角、图标路径解析、打开来源/链接等直接系统调用。把真实 Windows 行为留在 `veya-windows` 的小接口内，并避免 Tick 或 view 在无变化时重复系统查询。保留用户现有交互与错误反馈。

## 验收

- Iced 视图不直接操作 Win32 或 shell；系统动作的目标及参数保持原有语义。
- 稳定窗口下无高频重复圆角/图标解析；在 Windows 实际测试打开/复制/窗口行为。
- 只为当前真实调用建接口，不引入单适配器的通用框架。

## Implementation

- Added `veya-windows::platform::shell` (with an off-Windows stub) for opening
  source executables, validating/opening HTTP(S) links, and building Bing
  searches. `veya-desktop` no longer constructs `cmd /C start` commands or
  URL encodings.
- Changed rounded-window synchronization to return readiness and have the UI
  retry only during startup or after a maximize toggle, rather than querying
  the native window on every 250 ms tick. The Windows implementation keeps
  its geometry cache.
- Moved source executable path resolution out of the detail view. The
  selected source is resolved once in app state and the detail view reads the
  cached display path; unresolved sources retain the existing executable text.
- Verification: `cargo check --workspace` and `cargo test -p veya-desktop`
  passed (5 tests). An isolated Windows run confirmed the cached source path,
  a complex rounded region at 1080x700, no custom region while maximized, and
  restoration of the rounded region after returning to 1080x700. A real
  browser/link launch was not triggered during this run.
