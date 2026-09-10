## Common
common-ok = OK
common-cancel = Cancel
common-save = Save
common-saved = Saved
common-write-clipboard = Write to clipboard
common-plugin = Plugin
common-coming = Coming soon
common-clear = Clear
common-need-restart = Takes effect after restart
important-notice = Important notice

## App
app-name = Saladict
app-tagline = Rust + gpui-kit rewrite
app-translating = Translating…

## Translate window
translate-title = Saladict
translate-source-placeholder = Type or paste text to translate
translate-action = Translate
translate-swap = Swap languages
translate-copy = Copy result
translate-retry = Retry
translate-idle-hint = Enter text then press Translate
translate-error-prefix = Error

## Recognize window
recognize-title = Screenshot OCR
recognize-action = Recognize
recognize-copy = Copy result
recognize-to-translate = Translate
recognize-image-bytes = Image { $bytes } bytes
recognize-no-image = No image

## Config window
config-title = Preferences
config-nav-translate = Translate
config-nav-recognize = OCR
config-nav-tts = TTS
config-nav-collection = Collection
config-nav-hotkey = Hotkeys
config-nav-general = General

config-instances-count = { $title } · { $count } enabled
config-no-instances = No service enabled yet. Add one below.
config-add-instance = Add new service instance
config-add = Add
config-instance-name = Instance name
config-configure = Configure { $service }
config-remove = Remove
config-move-up = Move up
config-move-down = Move down
config-field-required = { $field } is required
config-field-unknown = Field

config-hotkey-hint = Global hotkeys (e.g. Command+T / Ctrl+Alt+X; leave empty to disable; applied on record, Backspace clears and unregisters)
config-hotkey-conflict = Hotkey conflict: this combo is already bound to "{ $name }"
config-hotkey-selection = Selection translate
config-hotkey-input = Input translate
config-hotkey-ocr = Screenshot OCR
config-hotkey-ocr-translate = Screenshot translate

config-clipboard-monitor = Monitor clipboard
config-dark-mode = Dark mode
config-transparent = Window translucency (new windows)
config-dock-icon = Hide Dock icon (macOS)
config-server-port = Server port (restart to apply)
config-dev-mode = Developer mode
config-nav-history = History
config-history-count = { $count } entries
config-history-empty = No translation history yet
config-history-clear = Clear history
config-proxy = HTTP proxy
config-proxy-host = Proxy host
config-proxy-port = Proxy port
config-save-settings = Save settings
config-saved-hint = Saved (some settings take effect after restart)

## Services
services-no-need = This service needs no configuration
services-instance-name = Instance name

## Tray
tray-selection-translate = Selection translate
tray-input-translate = Input translate
tray-ocr-recognize = Screenshot OCR
tray-ocr-translate = Screenshot translate
tray-clipboard-monitor = Monitor clipboard
tray-settings = Preferences
tray-quit = Quit

## Updater
updater-title = Check for updates
updater-download = Download
updater-checking = Checking…

## Tray/Notify/Updater additions
tray-error-menu-build = Menu build failed: { $err }
tray-error-icon-build = Tray icon creation failed: { $err }
tray-warn-not-init = Tray not initialized, cannot toggle clipboard monitor
tray-clipboard-off = Clipboard monitor stopped
tray-clipboard-captured = Clipboard captured: { $count } chars
tray-clipboard-on = Clipboard monitor started
tray-error-icon-gen = Icon generation failed: { $err }
notify-error-open = Failed to open notify window
notify-copied = Copied to clipboard
notify-hidden = Translate window hidden
updater-error-no-version = Failed to get version info
updater-latest-version = Latest version: { $version }
updater-hint = Click "Check for updates" to see if there's a new version
updater-window-title = Saladict · Update

## Task/window errors
task-cancelled = Task cancelled: { $error }
recognize-no-image-error = No image to recognize
recognize-no-service = No OCR service configured
recognize-window-title = Saladict · OCR

## Backup
config-nav-backup = Backup
config-backup-webdav-hint = WebDAV cloud backup (enter server URL and credentials, then upload/restore)
config-backup-server = Server URL
config-backup-username = Username
config-backup-password = Password
config-backup-to-cloud = Backup to cloud
config-backup-from-cloud = Restore from cloud
config-backup-local-hint = Local backup (save as zip file or restore from backup file)
config-backup-export = Export to file
config-backup-import = Import from file
config-backup-saved = Saved
config-proxy-username = Proxy username
config-proxy-password = Proxy password
config-no-proxy = Bypass proxy for

## 托盘补充
tray-auto-copy = Auto Copy
tray-copy-source = Source
tray-copy-target = Target
tray-copy-source-target = Source+Target
tray-copy-disable = Disable
tray-check-update = Check Update
tray-view-log = View Log
tray-restart = Restart
