# 04：平台资源生命周期与失败可见性

Status: ready-for-agent
Priority: P1
Blocked by: none
Implementation: 规划，未实施

## 根因与负责范围

`veya-windows/src/platform/win.rs::run` 在 `install_sender` 后依次注册 console handler、message window、clipboard listener 和 keyboard hook；这些步骤中的 `?` 可绕过尾部清理与 `release_sender`。worker 当前启动失败只输出 stderr；保留期设置也忽略 `set_setting`／purge 错误。负责 windows 的私有资源守卫及 worker 的对应错误状态。

## 方案与验收

- 各注册成功后由私有 RAII 所有者管理，任何后续失败都按正确线程与顺序释放 sender、窗口、listener、hook、hotkey 和 handler；不引入通用资源框架。
- 启动失败在 UI／可用入口可见，不遗留误报正常的后台记录状态；成功路径关闭与单例再启动保持正常。
- 保留期保存或清理失败提供准确状态，不能显示已成功并悄悄使用不同的持久设置；保存与清理的成功范围分开表达。
- 定向错误注入覆盖每个已注册资源后的失败，验证资源与 channel 生命周期；Win32 路径须在真实 Windows 检查，不能只做静态推断。

## Comments

若需改变保留策略或启动失败的产品行为，先向主 Agent 提供证据与选项；本票仅修复清理和错误反馈。
