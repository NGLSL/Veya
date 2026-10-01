# Veya Windows 实际验收

执行日期：2026-10-01（用户时区 America/Los_Angeles）。Windows 11 教育版，10.0.26200 / build 26200，单屏 1920×1080、100% 缩放。产品代码只读；控制夹具为本目录 `wincheck.py`、`launch.ps1`、`screenshot.ps1`，Python stdlib ctypes/sqlite3，无新增依赖。

## 构建与隔离

- baseline：`veya-baseline.exe`，主 Agent 提供的 HEAD `42d676c` 独立 worktree debug 编译；SHA-256 `5E664B984E77198DB9900A1A865189D686747D2971C81A550C47E11F03DDE73A`。
- 内存 candidate-v2：`veya-candidate-v2.exe`；SHA-256 `58BF3FA737724EA975A911EB74D6AFEB7D475A184BE4971E22A40749A8ADD748`。
- 完整行为 candidate-v3：`veya-candidate-v3.exe`；SHA-256 `B35216757117D66422796111B76847CC164DADE2FBCE344230529AC4C9A9E003`。
- baseline/candidate 均为同开发配置和工具链，默认 wgpu；没有用旧安装版本冒充基线。
- 行为数据：本目录 `appdata-behavior-v3/Veya/veya.db`，40 条独立文本 + 1 条不存在文件，后续增加合法文件和 2×2 RGBA PNG，共 43 条。
- 内存数据：从同一 `appdata-perf` 拷贝的 `appdata-perf-baseline` 与 `appdata-perf-current`，100 条记录，50×约 1 MiB 文本，50×2048² PNG；每张 PNG 69,631 bytes。
- 原安装运行实例 PID 12052 / `D:\Program Files\Veya\veya.exe` 在主 Agent 明确授权后按 PID 与精确路径核对并 Stop-Process 暂停，未删除或更改日常数据库。恢复状态见报告末尾。

## 实际用户路径结果（v3）

控制目标为独立 Python 进程 PID 23816 / `D:\Program Files\Python\python.exe` 的原生 Win32 EDIT 窗口，top-level HWND 5965918，EDIT HWND 11012834。夹具正常 WndProc 维护子控件焦点；测试前通过真实鼠标点击获取前台权限，临时 topmost 只用于显露宿主，唤起前已撤销。状态使用 EnumWindows/GetGUIThreadInfo，结果直接 WM_GETTEXT 读取目标文字。

| 操作 | 实际观察 |
| --- | --- |
| 原输入框 → Alt+V | 前台变为 Veya，实际窗口 440×580，搜索框可立即输入，首条选中 |
| Down → Enter | `ORIGINAL:` → `ORIGINAL:QUICK_FIXTURE_39`，面板隐藏；前台和 EDIT 焦点恢复原 HWND |
| 输入 `OUTSIDE_FIRST_PAGE_UNIQUE` → Enter | 搜索到第 1 条（不在首页），目标变成 `SEARCH:OUTSIDE_FIRST_PAGE_UNIQUE` |
| 鼠标点击首条 | 目标 `CLICK:` → `CLICK:QUICK_FIXTURE_40`，面板隐藏并恢复 EDIT 焦点 |
| Esc | 目标 `ESC:` 不变，面板隐藏，前台和 EDIT 焦点恢复 |
| 点击外部宿主失焦 | 面板隐藏，宿主保持前台，目标文字不变 |
| 搜索不存在文件 → Enter | 面板保持 440×580，显示路径不存在错误，目标 `MISSING:` 不变，没有把旧剪贴板内容贴进去 |
| 面板“管理历史” | 窗口恢复 1080×700 |
| 重复启动 v3 | 次进程 PID 29804 退出，已有实例仍为 PID 6568；原窗口激活并恢复 1080×700，没有第二窗口 |
| 系统托盘右键 → 设置 | 真实原生托盘菜单“设置”可点击，1080×700 管理窗口显示系统设置页；证据 `tray-settings-v3.png` |

同一行为库在三次文本重放后 raw count 仍为 41，paste_trigger 对应原记录 39、1、40，target `python.exe`；浏览器和记事本之后仍为 41，新增 paste_trigger 为原记录 1 → `chrome.exe`、原记录 38 → `Notepad.exe`。没有新增 Veya 自写 raw，历史 A 重放后使用线索归 A。

额外真实应用：

- Chrome：PID 18960，`C:\Program Files\Google\Chrome\Application\chrome.exe`，新建本目录 `chrome-profile` 隔离 profile，打开本地 `browser-target.html` textarea，Alt+V 搜索后 Enter。实际 HTML `input` 回调将标题变成 `VEYA_BROWSER_RESULT:OUTSIDE_FIRST_PAGE_UNIQUE - Google Chrome`，证明 textarea 实际收到完整文本；返回同 Chrome 窗口。没有访问用户登录态，没有 CDP。
- Windows Notepad：PID 28212，WindowsApps `Microsoft.WindowsNotepad_11.2607.14.0_x64__8wekyb3d8bbwe\Notepad\Notepad.exe`。启动时复用已有应用，先 Ctrl+N 新建空白测试 tab；只在该测试 tab 执行 Alt+V 搜索 `QUICK_FIXTURE_38` → Enter。WM_GETTEXT 与 UIAutomation TextPattern 双读均得到 `QUICK_FIXTURE_38`，前台及 EDIT 焦点恢复。原有用户 tab 内容未修改。
- typed file：合法文件记录重放成功，剪贴板 CF_HDROP 通过 DragQueryFileW 实读精确路径 `D:\Project\Veya\.scratch\veya-quick-paste\validation\replay-file.txt`；面板隐藏并回宿主。
- typed image：stdlib 生成合法 2×2 RGBA PNG，重放后 CF_DIB/CF_DIBV5 均存在，CF_DIB 读取 header 显示 width=2、height=-2、bitcount=32，面板隐藏并回宿主。原生 EDIT 不支持图片/文件插入，因此这里只证明系统剪贴板 typed replay，未声称目标插入图片/文件成功。

单窗预览：`quick-panel-v3.png`，只裁取 GetWindowRect 440×580，不包含桌面；缺失文件与状态记录见 `behavior-v3.json`。

## 内存实际对照（baseline 与 v2）

同一数据库夹具、默认 wgpu、同 1080×700 管理窗口，Get-Process PrivateMemorySize64 / WorkingSet64，单次运行。每次状态操作后约 0.7 秒取样；启动后的 image management 取样在 UI 已绘制之后。所有值是 bytes，非编译静态估算。

| 状态 | baseline Private / WS | v2 Private / WS |
| --- | --- | --- |
| 图片管理首页 | 304,132,096 / 269,373,440 | 263,946,240 / 212,615,168 |
| 长文本选中 | 305,319,936 / 270,585,856 | 265,003,008 / 213,745,664 |
| 暂停/恢复两次 | 305,336,320 / 271,360,000 | 265,003,008 / 213,766,144 |
| 隐藏到托盘 | 279,076,864 / 245,075,968 | 251,420,672 / 200,114,176 |

该单次夹具管理首页 Private 约下降 13.2%，WS 约下降 21.1%。状态更新时 v2 Private 保持，baseline 略升；这只能支持本场景实测，不代表所有机器/数据库/长时间使用都有相同降幅。v2 快捷面板单独取样 291,377,152 / 239,824,896，尺寸切换及渲染缓存导致其不能直接与管理首页比较。v3 的写入锁修复不改变本次测量的历史页面缓存代码，但未对 v3 重跑性能矩阵。原始数据见 `baseline-memory.json`、`current-memory.json`。

## 查出的运行问题及复核

SetClipboardData 后仍持有写锁时 clipboard sequence 与 CloseClipboard 后不同：before 1377，写锁内 1379，Close 后 1382；随后只读 OpenClipboard/GetClipboardData 得到相同文本，读锁内及 Close 后均为 1382。此证据促成平台两阶段序号确认。

v2 Enter 实际出现 `0x8007058A`（线程没有打开的剪贴板），保持面板且自写记录被捕获。报告给平台实现方后，v3 串行 listener/write 的本进程剪贴板锁；以上文本、文件、图片与无新增 raw 真实结果均来自 v3，先前 v2 失败已不能代表修复后的行为。

初版 STATIC 宿主不维护 EDIT 焦点，以及重启宿主后读取旧 host.json 的夹具竞态，已排除；这些早期结果不作为产品失败证据。

## 限制

未验证提权目标/UIPI、目标关闭或 clipboard 被第三方替换的故障注入、不同显示器/缩放、长时间资源曲线、图片/文件实际目标插入、VSCode 编辑器场景。内存测试是 debug 单次对照，没有 release/安装性能或稳定分位统计。未发布、未覆盖安装版本。

## 清理与最终候选

最终 v4：`veya-candidate-v4.exe`，SHA-256 `B77B650140A0B71AA3B50D298723887C49C48BA7F838500BD13D21A98FE98F99`。v4 只对旧历史执行一次 Alt+V → 搜索 `QUICK_FIXTURE_37` → Enter；真实目标从 `FINAL:` 变为 `FINAL:QUICK_FIXTURE_37`，面板隐藏，前台 HWND5965918/EDIT11012834 恢复，raw count 43 → 43。记录 `final-v4-smoke.json`。完整矩阵来自 v3，v4 不重复矩阵；历史缓存的内存矩阵来自 v2。

v3 Windows VersionInfo 的 FileDescription / ProductName 实读均为 `Veya`；没有以截图猜测任务管理器分组行为。根 Agent 回报最终 workspace 95 tests、fmt/check/build/diff 通过；这些自动门禁由根 Agent 执行，不替代本报告的运行证据。第三方读锁导致 partial write 的故障路径自动覆盖，未进行真实故障注入。

清理已完成：Notepad 只关闭本次新建 `QUICK_FIXTURE_38` tab，在明确测试内容的保存提示点“不保存”，原用户 tabs 保留并恢复原隐藏状态；隔离 Chrome HWND134912 正常 WM_CLOSE，原生宿主 HWND5965918 正常 WM_CLOSE。候选 v4 PID30044 退出前再次核对精确测试路径后 Stop-Process；Get-Process 确认测试 Chrome PID18960、宿主 PID23816、候选 PID30044 均已退出。原安装 `D:\Program Files\Veya\veya.exe` 已用原 APPDATA 恢复，最终 PID24436，并通过原 Alt+V 热键恢复原隐藏到托盘状态。首次恢复实例 PID23224 的 WM_CLOSE 实际退出后，重新启动得到最终 PID24436；最终 Get-Process 与 EnumWindows 均确认路径正确、运行且窗口隐藏。所有测试数据库和夹具留本目录，可复核；没有删日常数据库或停止其它用户进程。
