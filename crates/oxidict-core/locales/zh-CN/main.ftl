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

config-hotkey-hint = 全局快捷键（如 Command+T / Ctrl+Alt+X，留空禁用；录入即生效，Backspace 清空并注销）
config-hotkey-conflict = 快捷键冲突：该组合键已被「{ $name }」占用
config-hotkey-selection = 划词翻译
config-hotkey-input = 输入翻译
config-hotkey-ocr = 截图 OCR
config-hotkey-ocr-translate = 截图翻译

config-clipboard-monitor = 监听剪切板
config-dark-mode = 深色模式
config-transparent = 窗口毛玻璃（新开窗口生效）
config-dock-icon = 隐藏 Dock 图标（macOS）
config-server-port = 服务器端口（重启生效）
config-dev-mode = 开发者模式
config-nav-history = 历史记录
config-history-count = 共 { $count } 条记录
config-history-empty = 暂无翻译历史
config-history-clear = 清空历史
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
notify-copied = 已复制到剪贴板
notify-hidden = 翻译窗口已隐藏
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
config-backup-webdav-hint = 选择备份位置：WebDAV 服务器，或本机目录（可指向 iCloud/OneDrive 等同步目录）
config-backup-server = 服务器地址
config-backup-username = 用户名
config-backup-password = 密码
config-backup-to-cloud = 立即备份
config-backup-from-cloud = 从备份恢复
config-backup-local-hint = 本地备份（保存为 zip 文件或从备份文件恢复）
config-backup-export = 导出到文件
config-backup-import = 从文件导入
config-backup-saved = 已保存
config-proxy-username = 代理用户名
config-proxy-password = 代理密码
config-no-proxy = 不走代理的主机

## 托盘补充
tray-auto-copy = 自动复制
tray-copy-source = 原文
tray-copy-target = 译文
tray-copy-source-target = 原文+译文
tray-copy-disable = 关闭
tray-check-update = 检查更新
tray-view-log = 查看日志
tray-restart = 重启应用

## 恢复确认 / 历史检索 / 外观
config-backup-empty = 没有可用的备份文件
config-backup-pick = 恢复此备份
config-restore-confirm-remote = 将从云端备份 { $name } 恢复，并覆盖当前配置与历史
config-restore-confirm-local = 将从配置目录的 oxidict-backup.zip 恢复，并覆盖当前配置与历史
config-restore-snapshot-note = 覆盖前会把现有配置与历史另存为 *.pre-restore.bak，可据此回退。
config-restore-confirm = 确认覆盖恢复
config-history-search = 搜索原文或译文
config-history-no-match = 没有匹配的历史记录
config-history-copy = 复制译文
config-language = 界面语言
config-theme = 主题
config-theme-system = 跟随系统
config-theme-light = 浅色
config-theme-dark = 深色

## 备份/恢复的操作反馈（状态行文案）
config-backup-uploaded = 已保存备份: { $name }
config-backup-upload-failed = 备份失败: { $error }
config-backup-list-fetching = 正在获取备份列表…
config-backup-list-count = 云端共 { $count } 份备份，请选择要恢复的一份
config-backup-list-failed = 获取备份列表失败: { $error }
config-backup-export-done = 已导出到配置目录 oxidict-backup.zip
config-backup-export-failed = 导出失败: { $error }
config-restore-restoring = 正在恢复 { $name }…
config-restore-done = 已从备份恢复 { $name }；原数据已另存为 *.pre-restore.bak
config-restore-done-local = 已从本地备份恢复；原数据已另存为 *.pre-restore.bak
config-restore-failed = 恢复失败: { $error }
config-hotkey-placeholder = 点击后按组合键

## 备份后端选择
config-backup-type = 备份位置
config-backup-type-webdav = WebDAV 服务器
config-backup-type-local = 本地目录
config-backup-folder = 备份目录
config-backup-uploading = 正在备份: { $name }

## 界面字体
config-font = 界面字体
config-font-size = 界面字号
config-font-system = 留空使用系统字体

## 自定义服务
config-nav-custom = 自定义服务
config-custom-intro = 加一个"单次请求 + 从 JSON 取字段"的翻译服务：填完保存即生效，无需重启。复杂情形（多步请求、签名、HTML 解析）请用 .potext 插件。
config-custom-id = 服务标识
config-custom-endpoint = 接口地址
config-custom-method = 请求方法
config-custom-auth = 鉴权方式
config-custom-auth-name = 参数名
config-custom-body = 请求体模板（JSON，可空）
config-custom-extract = 译文路径
config-custom-tls = HTTP 指纹
config-custom-save = 保存并生效
config-custom-delete = 删除
config-custom-list = 已配置
config-custom-empty = 还没有自定义服务
config-custom-saved = 已保存并生效
config-custom-auth-none = 无需鉴权
config-custom-auth-bearer = Bearer 令牌
config-custom-auth-header = 自定义请求头
config-custom-auth-query = 查询参数

## 分页 / 拖拽兜底 / 自定义服务编辑
config-history-more = 加载更多
config-move-to-end = 拖到此处移到最后
config-custom-edit = 编辑
config-custom-editing = 已回填到表单，修改后点「保存并生效」
config-custom-edit-unsupported = 签名类条目不支持表单编辑，请在 config.json 中修改
