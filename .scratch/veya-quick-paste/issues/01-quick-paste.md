# 01：快捷粘贴闭环

Status: ready-for-human
Priority: P1
Blocked by: none
Implementation: 实现完成；自动检查与 Windows 核心路径通过，未验证组合见报告

## 负责范围

`veya-desktop/src/app.rs`、`app/quick.rs`、`worker.rs`、`capture.rs`；`veya-windows/src/platform/` 的目标恢复和 typed clipboard replay；必要的 core replay 关联逻辑。保持现有管理窗口、托盘、原记录身份和 confidence 契约。

## 验收

- 默认 Alt+V 打开约 440×580 面板，最近优先，有明确选中项，上下键、搜索、Enter 和点击可用；搜索跨完整历史。
- 文本、图片、文件列表按原类型重放；写入成功且序号仍匹配时，收起面板、恢复原目标并发送 Ctrl+V。
- 写入失败、目标失效或剪贴板竞争时不误粘旧内容；错误可见并可恢复。
- Esc／失焦关闭，取消后原应用可继续输入；管理入口恢复管理查询和窗口状态。
- 内部重放不新增 raw record，后续 paste attempt 关联原记录；不将观察到的快捷键宣称为插入成功。
- 记事本、浏览器和常用编辑器的实际 Windows 操作有可审阅证据；文件和图片用支持的目标分别验证，未验证组合明确列出。

## 验证记录

结果归档至 [运行报告](../validation/report.md)。实际验证了原生 EDIT、记事本、浏览器文本插入，搜索、鼠标、Esc、失焦、管理、托盘和单例路径；文件／图片验证了原格式剪贴板重放。提权目标、多屏和长期资源曲线等未验证组合在报告中列出。

## Comments

用户确认快捷粘贴方向，并要求将整体代码优化及不降低体验的内存优化纳入规划。
