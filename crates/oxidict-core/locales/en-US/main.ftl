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
app-name = Oxidict
app-tagline = Rust + gpui-kit rewrite
app-translating = Translating…

## Translate window
translate-title = Oxidict
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
updater-window-title = Oxidict · Update

## Task/window errors
task-cancelled = Task cancelled: { $error }
recognize-no-image-error = No image to recognize
recognize-no-service = No OCR service configured
recognize-window-title = Oxidict · OCR

## Backup
config-nav-backup = Backup
config-backup-webdav-hint = Choose where backups go: a WebDAV server, or a local folder (e.g. an iCloud/OneDrive sync folder)
config-backup-server = Server URL
config-backup-username = Username
config-backup-password = Password
config-backup-to-cloud = Backup now
config-backup-from-cloud = Restore from backup
config-backup-local-hint = Local backup (save as zip file or restore from backup file)
config-backup-export = Export to file
config-backup-import = Import from file
config-backup-saved = Saved
config-proxy-username = Proxy username
config-proxy-password = Proxy password
config-no-proxy = Bypass proxy for

## Tray (supplement)
tray-auto-copy = Auto Copy
tray-copy-source = Source
tray-copy-target = Target
tray-copy-source-target = Source+Target
tray-copy-disable = Disable
tray-check-update = Check Update
tray-view-log = View Log
tray-restart = Restart

## Restore confirmation / history search / appearance
config-backup-empty = No backups available
config-backup-pick = Restore this backup
config-restore-confirm-remote = Restore from cloud backup { $name }, overwriting the current config and history
config-restore-confirm-local = Restore from oxidict-backup.zip in the config directory, overwriting the current config and history
config-restore-snapshot-note = The current config and history are saved as *.pre-restore.bak before overwriting, so you can roll back.
config-restore-confirm = Confirm restore
config-history-search = Search source or translation
config-history-no-match = No matching history
config-history-copy = Copy translation
config-language = Interface language
config-theme = Theme
config-theme-system = Follow system
config-theme-light = Light
config-theme-dark = Dark

## Backup/restore status messages
config-backup-uploaded = Backup saved: { $name }
config-backup-upload-failed = Backup failed: { $error }
config-backup-list-fetching = Fetching backup list…
config-backup-list-count = { $count } backups in the cloud; pick one to restore
config-backup-list-failed = Failed to fetch backup list: { $error }
config-backup-export-done = Exported to oxidict-backup.zip in the config directory
config-backup-export-failed = Export failed: { $error }
config-restore-restoring = Restoring { $name }…
config-restore-done = Restored from backup { $name }; the previous data was saved as *.pre-restore.bak
config-restore-done-local = Restored from the local backup; the previous data was saved as *.pre-restore.bak
config-restore-failed = Restore failed: { $error }
config-hotkey-placeholder = Click, then press keys

## Backup backend selection
config-backup-type = Backup location
config-backup-type-webdav = WebDAV server
config-backup-type-local = Local folder
config-backup-folder = Backup folder
config-backup-uploading = Backing up: { $name }

## Interface font
config-font = Interface font
config-font-size = Interface font size
config-font-system = Leave empty to use the system font

## Custom services
config-nav-custom = Custom services
config-custom-intro = Add a "single request + extract from JSON" translation service: save and it takes effect immediately, no restart. For complex cases (multi-step requests, signing, HTML parsing) use a .potext plugin.
config-custom-id = Service id
config-custom-endpoint = Endpoint
config-custom-method = Method
config-custom-auth = Authentication
config-custom-auth-name = Parameter name
config-custom-body = Request body template (JSON, optional)
config-custom-extract = Translation path
config-custom-tls = HTTP fingerprint
config-custom-save = Save and apply
config-custom-delete = Delete
config-custom-list = Configured
config-custom-empty = No custom services yet
config-custom-saved = Saved and applied
config-custom-auth-none = No authentication
config-custom-auth-bearer = Bearer token
config-custom-auth-header = Custom header
config-custom-auth-query = Query parameter

## Pagination / drag fallback / custom service editing
config-history-more = Load more
config-move-to-end = Drop here to move to the end
config-custom-edit = Edit
config-custom-editing = Loaded into the form; click "Save and apply" after editing
config-custom-edit-unsupported = Signed entries can't be edited in the form; edit config.json instead
