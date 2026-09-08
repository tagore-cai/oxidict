## 通用
common-ok = 确定
common-cancel = 取消
common-save = 保存
common-saved = 已保存
common-write-clipboard = 写入剪切板
common-plugin = 插件
common-coming = 敬请期待
common-clear = 清空
common-need-restart = 需要重启生效
important-notice = 重要提示

## 应用
app-name = 沙拉翻译
app-tagline = Rust + gpui-kit 重写版
app-translating = 翻译中…

## 翻译窗口
translate-title = 沙拉翻译
translate-source-placeholder = 输入或粘贴要翻译的文本
translate-action = 翻译
translate-swap = 交换语言
translate-copy = 复制结果
translate-retry = 重试
translate-idle-hint = 输入文本后点击翻译
translate-error-prefix = 出错

## 识别窗口
recognize-title = 截图识别
recognize-action = 开始识别
recognize-copy = 复制结果
recognize-to-translate = 翻译
recognize-image-bytes = 图片 { $bytes } 字节
recognize-no-image = 暂无图片

## 设置窗口
config-title = 偏好设置
config-nav-translate = 翻译服务
config-nav-recognize = 识别服务
config-nav-tts = 语音合成
config-nav-collection = 生词本
config-nav-hotkey = 快捷键
config-nav-general = 通用

config-instances-count = { $title } · 已启用 { $count } 个实例
config-no-instances = 尚未启用任何服务，从下方添加。
config-add-instance = 添加新服务实例
config-add = 添加
config-instance-name = 实例名称
config-configure = 配置 { $service }
config-remove = 移除
config-move-up = 上移
config-move-down = 下移
config-field-required = { $field } 不能为空
config-field-unknown = 字段

config-hotkey-hint = 全局快捷键（如 Command+T / Ctrl+Alt+X，留空禁用；修改后需保存）
config-hotkey-selection = 划词翻译
config-hotkey-input = 输入翻译
config-hotkey-ocr = 截图 OCR
config-hotkey-ocr-translate = 截图翻译

config-clipboard-monitor = 监听剪切板
config-dark-mode = 深色模式
config-proxy = HTTP 代理
config-proxy-host = 代理主机
config-proxy-port = 代理端口
config-save-settings = 保存设置
config-saved-hint = 已保存（部分设置重启后生效）

## 服务
services-no-need = 该服务无需配置
services-instance-name = 实例名称

## 托盘
tray-selection-translate = 划词翻译
tray-input-translate = 输入翻译
tray-ocr-recognize = 截图 OCR
tray-ocr-translate = 截图翻译
tray-clipboard-monitor = 监听剪切板
tray-settings = 偏好设置
tray-quit = 退出

## 更新
updater-title = 检查更新
updater-download = 前往下载
updater-checking = 检查中…

## 托盘/通知/更新 补充
tray-error-menu-build = 菜单组装失败: { $err }
tray-error-icon-build = 托盘图标创建失败: { $err }
tray-warn-not-init = 托盘尚未初始化，无法切换剪切板监听
tray-clipboard-off = 剪切板监听已关闭
tray-clipboard-captured = 剪切板捕获: { $count } 字符
tray-clipboard-on = 剪切板监听已开启
tray-error-icon-gen = 图标生成失败: { $err }
notify-error-open = 打开通知窗口失败
updater-error-no-version = 未获取到版本信息
updater-latest-version = 最新版本：{ $version }
updater-hint = 点击「检查更新」查看是否有新版本
updater-window-title = 沙拉翻译 · 更新

## 任务/窗口错误
task-cancelled = 任务被取消: { $error }
recognize-no-image-error = 没有可识别的图片
recognize-no-service = 未配置识别服务
recognize-window-title = 沙拉翻译 · 识别

## 备份
config-nav-backup = 备份
config-backup-webdav-hint = WebDAV 云端备份（填入服务器地址与凭据后可上传/恢复）
config-backup-server = 服务器地址
config-backup-username = 用户名
config-backup-password = 密码
config-backup-to-cloud = 备份到云端
config-backup-from-cloud = 从云端恢复
config-backup-local-hint = 本地备份（保存为 zip 文件或从备份文件恢复）
config-backup-export = 导出到文件
config-backup-import = 从文件导入
config-backup-saved = 已保存
config-proxy-username = 代理用户名
config-proxy-password = 代理密码
config-no-proxy = 不走代理的主机
