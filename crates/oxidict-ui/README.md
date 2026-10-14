# oxidict-ui

**GPUI 窗口层。** 本工程第二大 crate（6,219 行 / 14 文件）。

> 内部结构、逐文件职责、不变量清单与设计决策 → [ARCHITECTURE.md](./ARCHITECTURE.md)

职责边界：**只做渲染与交互编排**。服务选择、历史写入、平台调用都在 `services` / `platform` /
`app` 里。**没有任何 crate 依赖本包**（只有 `app`）——窗口层是终端消费者。

## 依赖

```
oxidict-ui  →  oxidict-core, oxidict-services, oxidict-net, oxidict-platform, gpui-kit, tray-icon
被依赖：app
```

## 窗口清单

| 窗口 | 文件 | 行数 | 说明 |
| --- | --- | --- | --- |
| 主翻译窗口 | `translate_window.rs` | 627 | 源文本区 + 语言选择 + 多实例结果卡片（Idle/Loading/Done/Failed 四态）+ 流式增量上屏 |
| 设置窗口 | `config_window/{mod,pages,services}.rs` | 2968 | 9 页：翻译 / 识别 / TTS / 生词本 / 快捷键 / 通用 / 备份 / 历史 / 自定义服务 |
| 识别窗口 | `recognize.rs` | 301 | 截图 OCR（图片区 + 结果区 + 开始识别 / 复制结果） |
| 更新窗口 | `updater.rs` | 246 | 当前版本 + 检查 GitHub release + 前往下载 |
| 输入翻译 | `input_translate.rs` | 215 | 独立小窗（320×180），Enter 翻译、Esc 关闭 |
| 通知窗 | `notify.rs` | 109 | always-on-top 小窗，3 秒自动关闭。**最薄的一个，新窗口照它抄** |

基础设施：`lib.rs`（窗口创建 / 主题 / 命令循环）、`tray.rs`（托盘 + 菜单）、
`hotkey.rs`（快捷键录制）、`tokens.rs`（排版标尺）、`logos.rs`（品牌资产）。

## 托盘菜单

划词翻译 / 输入翻译 / OCR 识别 / OCR 翻译 / 剪切板监听（勾选）/ 自动复制（单选组：
原文 / 译文 / 原文+译文 / 关闭）/ 偏好设置 / 检查更新 / 查看日志（仅 `dev_mode`）/ 重启 / 退出。

所有菜单项都只发 `TrayCommand`，真正的状态变更在 GPUI 命令循环里执行。

## 配置项覆盖范围

界面读取的配置键覆盖：窗口几何与置顶、`translate_*` 行为族（动态翻译 / 增量翻译 / 去换行 /
字号 / 回车还原 / 第二语言 / 关窗行为 / 自动复制）、`recognize_*` 行为族、代理（含认证）、
`app_theme` / `app_font` / `app_font_size` / `app_language`、`transparent` / `hide_dock_icon`、
备份（WebDAV / 本地目录）、快捷键 × 4、历史、自定义服务。

唯一**读但不生效**的键是 `app_fallback_font`（gpui 主题只暴露单一 `font_family`，没有候选列表）。

## 三个 GPUI 特有的坑

写窗口代码前值得先知道（细节见 [ARCHITECTURE.md §8](./ARCHITECTURE.md)）：

1. **Esc 关窗必须用元素级 `.on_key_down`**，不能用 `window.on_key_event`——后者只能在 paint 阶段
   调用，而 `render` 跑在 layout 阶段，在 render 里调用会在启动时 panic。
2. **光标坐标域因平台而异**：macOS 是 points（与 gpui 逻辑坐标同域），Windows/Linux 是物理像素
   需除以 `scale_factor`。
3. **每个新窗口开窗前必须恢复主题与字体**（`apply_saved_theme`）。漏掉的话
   `gpui_component::init` 会把主题强制设成 Light，用户保存的 `app_theme` 被静默忽略。

## 排版与字体

应用自有文字统一走 `tokens.rs` 的具名档位（未缩放值）：

| 档位 | px | 用途 |
| --- | --- | --- |
| `Xs` | 10 | 侧栏极小字 |
| `Sm` | 11 | 次要小字 |
| `Body` | 12 | 说明 / 辅助文本 / 数据行 |
| `Label` | 13 | 设置项标签 |
| `Heading` | 14 | 页标题 |
| `Logo` | 16 | 侧栏 Logo 字符 |

`app_font_size = 16` 时缩放系数恰为 1，**不产生任何视觉变化**（档位值就是改造前手写的像素值）。
缩放夹在 `[0.75, 1.6]` 防止手改配置把布局撑爆。

## 测试

本 crate 有 **17 个测试**：

| 文件 | 数量 | 覆盖 |
| --- | --- | --- |
| `logos.rs` | 8 | logo 嵌入非空、SPECIAL 表指向存在的文件、system 无 logo、id → 文件名映射、png 回退、未知服务返回 `None` |
| `hotkey.rs` | 6 | 必须有修饰键、修饰键组合、特殊键与 F 键、拒绝未知/越界、标点与数字键盘、清空键 |
| `tokens.rs` | 2 | 档位顺序与像素值、缩放系数与夹取 |
| `config_window/mod.rs` | 1 | 字号配置值 → 下拉下标 |

```bash
cargo test -p oxidict-ui
```

窗口渲染本身无法在无窗口环境验证，靠实机运行覆盖。

## 已知限制

- **识别窗口的图片区未接入渲染**：当前显示占位框 + 文件大小。gpui 的 `img()` 可用，
  **缺的是把截图字节转成 `ImageSource` 的接入**——最明确的待补点。
- **识别结果回填翻译未实现**（结果可复制，但没有「送去翻译」按钮）。
- **历史页无真正分页**：`HISTORY_PAGE = 100` 是「加载更多」的步长。
- **通知窗的 3 秒时长硬编码**，未做配置。
- **改语言不会更新已打开窗口的标题栏**（标题在创建时取一次）。
- **托盘状态用 `AtomicPtr` 常驻**（`CheckMenuItem` 非 `Send`/`Sync`），
  其正确性依赖「只在主线程访问」这条约定，没有编译期保障。
