# 07：隐藏窗口 CPU 空转

Status: ready-for-human
Priority: P1
Implementation: Windows runtime 到期重绘等待已修补，隔离 CPU 回归通过

## 现象与根因

安装文件 SHA-256 与当天本地 release 构建一致，版本资源仍是 0.2.1；不能据此判定用户未更新。安装进程隐藏时持续约 6.1%，16 个逻辑核下接近一核满载；显示快捷面板降到 0.26%，再隐藏回到 5.92%。热线程 RIP 100 次采样 69 次落在 `NtUserRedrawWindow`，其余主要是 Iced/winit 事件循环；平台、数据库和单实例线程等待。

空历史库也触发回归，移除隐藏界面控件不解决。Iced 0.13.0 在 `ResumeTimeReached` 中请求重绘，却只在随后 `RedrawRequested` 中设置新的等待；隐藏后的过期 `WaitUntil` 会反复产生到期事件。

## 修改与契约

只在 `vendor/iced_winit/src/program.rs` 的 Windows Init／ResumeTimeReached 分支，重绘前排入 `ChangeFlow(Wait)`，清退到期等待。原 runner 保留未来 deadline 的保护；可见重绘继续安装后续动画期限。其余上游源码保持原样，保留 MIT license；根 manifest 用 patch 选用此副本，不修改 registry 缓存。

不改变剪贴板追踪、快捷键、UI 轮询间隔、渲染器、数据格式和历史内容。修补只针对当前 Windows 单窗口产品，不增加多窗口或调度抽象。

## 验收

同机、相同空历史库、相同 Alt+V 收起流程，基线 CPU 0.71／2.26／5.97%，修补后 release 0.047／0.097／0.048%，开发候选 0.094／0／0.048%。每段约两秒、全程检查窗口隐藏，三段均低于 1% 才通过。可见光标与真实重开粘贴验收另见 [CPU 报告](../validation/cpu-report.md)。

## Comments

用户报告最新安装版本后台仍在 6.1%；已核对安装构建哈希，确认问题存在于最新构建。单次任务管理器截图不是完整性能曲线，以上为当前环境的短时实测。
