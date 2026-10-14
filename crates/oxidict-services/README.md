# oxidict-services

**四大类服务的抽象与全部内置实现。**

> 内部结构、逐文件职责、不变量清单与设计决策 → [ARCHITECTURE.md](./ARCHITECTURE.md)

原实现中服务是一组 JS 模块靠 `index.jsx` 命名空间聚合；
这里改为 **trait + 注册表**，内置服务与 `.potext` 插件注册到同一张表里，调用方
（UI、本地 HTTP 服务、CLI）**不区分二者**。

## 依赖

```
oxidict-services  →  oxidict-core, oxidict-net, oxidict-platform
被依赖：oxidict-plugin（适配器实现这些 trait）、oxidict-ui、app
```

`→ oxidict-platform` 是受控例外：`recognize/system`（系统 OCR）、`tesseract` 等内置服务确实要调
系统能力。platform 层本身不依赖 services，方向未反。

外部依赖：`async-trait`、`wreq`、`scraper`（CSS 选择器）、`image` + `rqrr`（二维码）、
`hmac`/`sha1`/`sha2`/`md-5`（服务签名）、`futures`（流式）、`serde_json`。

## 四个 trait

四个 trait 结构同构——`id()` / `name()` / `config_schema()` / `map_language()` + 一个异步调用方法：

```rust
#[async_trait]
pub trait Translator: Send + Sync {
    fn id(&self) -> &str;
    fn name(&self) -> &str { self.id() }                       // 默认即 id
    fn config_schema(&self) -> Vec<ConfigField> { vec![] }      // 空 = 免配置
    fn map_language(&self, lang: Language) -> String { lang.code().to_string() }
    async fn translate(&self, req: TranslateRequest) -> Result<TranslateResult>;
}
```

| trait | 方法 | 返回 |
| --- | --- | --- |
| `Translator` | `translate` | `TranslateResult`（纯文本 或 结构化词典） |
| `Recognizer` | `recognize` | `String` |
| `Tts` | `tts` | `Vec<u8>`（mp3） |
| `Collector` | `collect` | `()`（生词本，写远程） |

**`map_language()` 只在厂商语言码与内部码不一致时才需要覆写。** 默认直接透传内部码，
所以「内部码 → 厂商码」的偏差项在各服务文件里是显式的。

**`config_schema()` 决定设置面板长什么样。** 返回 `Vec<ConfigField>`（`text`/`secret`/`select`/
`switch`/`number`），设置面板据此渲染——原版 30 份手写 `Config.jsx` 由它归一。

## 注册表

```rust
static SERVICES: OnceCell<Services> = OnceCell::new();
pub fn services() -> &'static Services;

// 注册 / 查询
services.register_translator(Arc::new(Google));
services.translator("google") -> Option<Arc<dyn Translator>>
services.translators()        -> Vec<Arc<dyn Translator>>
services.remove_translator(id) -> bool     // 自定义服务改配置后即时生效用

// 实例解析：`alibaba@r4nd0m` → (服务实现, 该实例的配置)
services.resolve_translator(instance, &store) -> Result<(Arc<dyn Translator>, ServiceConfig)>
```

`remove_translator` 的存在理由：没有它，用户删掉或改名一条自定义服务后，旧实现会一直留在注册表里
直到重启。

### 面向 UI 的异步入口

```rust
translate_instance(instance, req) -> Result<TranslateResult>   // 指定实例
recognize_instance(instance, req) -> Result<String>
tts_instance(instance, req) -> Result<Vec<u8>>

translate_first_enabled(&store, req) -> Result<(String, TranslateResult)>          // async
translate_first_enabled_blocking(req) -> Result<(String, TranslateResult)>         // 阻塞版

spawn_translate(instance, req) -> JoinHandle<...>   // 调度到全局 tokio runtime
spawn_recognize / spawn_tts
```

`translate_first_enabled*` 是**唯一**的「读启用列表 → 取首个实例 → resolve → translate」实现：
CLI（`--translate`）、本地 HTTP 服务、托盘三条入口过去各写了一遍，任何改动都要同步三处。
现在收敛为一处，返回 `(实例 key, 结果)` 便于调用方写历史库。

> `translate_first_enabled_blocking` 用 `runtime::handle().block_on`。调用线程若已在 tokio runtime
> 上下文内会 panic。现有三个调用点都在独立 `std::thread` 上，不受影响。

`spawn_*` 返回的 `tokio::JoinHandle` **可以在 GPUI 执行器上直接 await**（JoinHandle 不依赖 tokio
线程局部）——这是 GPUI 与 tokio 服务层之间的标准桥。

## 服务目录（50 个）

服务 id 即展示名（`name()` 默认返回 `id()`）；界面上每个 id 的 logo 由
`oxidict-ui::logos::logo_uri` 映射到 `crates/oxidict-ui/assets/logos/` 下的图片。

### 翻译 30 个

**独立实现 21 个**（`translate/<name>.rs`，`register()` 里逐个注册）：

| 文件 | id | 备注 |
| --- | --- | --- |
| `alibaba.rs` | `alibaba` | 阿里云，签名走 `alibaba_escape` + HMAC-SHA1 |
| `baidu.rs` | `baidu` | 百度通用翻译 |
| `baidu_field.rs` | `baidu_field` | 百度领域模型翻译 |
| `bing.rs` | `bing` | 微软翻译 |
| `bing_dict.rs` | `bing_dict` | 必应词典（结构化结果） |
| `caiyun.rs` | `caiyun` | 彩云小译 |
| `cambridge_dict.rs` | `cambridge_dict` | 剑桥词典，HTML + CSS 选择器提取 |
| `chatglm.rs` | `chatglm` | 智谱 GLM 官方 API |
| `deepl.rs` | `deepl` | DeepL 官方 API |
| `ecdict.rs` | `ecdict` | 离线 ECDICT 词典 |
| `geminipro.rs` | `geminipro` | Gemini 官方 API（复用 `openai_compatible`） |
| `google.rs` | `google` | 谷歌翻译 |
| `lingva.rs` | `lingva` | Lingva 自建实例 |
| `niutrans.rs` | `niutrans` | 小牛翻译 |
| `ollama.rs` | `ollama` | 本地 Ollama（复用 `openai_compatible`） |
| `openai.rs` | `openai` | OpenAI 官方 API（复用 `openai_compatible`） |
| `tencent.rs` | `tencent` | 腾讯云 TMT，签名走 TC3 |
| `transmart.rs` | `transmart` | 腾讯交互翻译 |
| `volcengine.rs` | `volcengine` | 火山引擎，签名走 AWS SigV4 |
| `yandex.rs` | `yandex` | Yandex 翻译 |
| `youdao.rs` | `youdao` | 有道智云 |

**表驱动 9 个**（`translate/relay.rs` 的 `RELAYS` 静态表 + `Relay` 执行器）：

`openai_cloud` / `claude_cloud` / `gemini_cloud` / `chatglm_cloud` / `deepseek_cloud` /
`minimax_cloud` / `moonshot_cloud` / `tongyi_cloud` / `deepl_cloud`

改造前这 9 个服务各占一个文件（合计约 1600 行），实测**每个与 `openai_cloud.rs` 只差 23–24 行**，
且那些行全是文档注释、结构体名、`DEFAULT_MODEL`、`MODELS`、`id()` 与错误串——真正的差异只有**数据**。

> **注意**：`deepl_cloud` 的请求体形态与其余 8 个不同（单条不含 `role` 的消息，`content` 是
> `{"text","sl","tl"}` 的 JSON 串，无 `temperature`），由 `BodyShape::DeeplRelay` 区分。
> `id` 会写进用户的 `config.json`，**不可随意改名**——`relay.rs` 里有测试把守。

**为什么只看数据而不上通用引擎**：变换器只提供具名枚举（`BodyShape` / `FieldShape`），
**刻意不引入**通用表达式或脚本语言。需要任意逻辑（多步请求、签名、解析 HTML）时走 `.potext` 插件，
而不是在这里造一门小语言。

**非注册模块 2 个**：

- `openai_compatible.rs` — OpenAI 协议共享实现层（消息构造、URL 规范化、请求体组装），
  被 `openai` / `ollama` / `geminipro` 复用。**不是可注册服务**，故收紧为 `pub(crate)`。
- `relay.rs` — 上表的执行器本身。
- `custom.rs` — 见下节。
- `lang_names.rs` — 语言英文名辅助（`{{from}}`/`{{to}}` 占位符用）。

### 配置驱动的自定义服务（`translate/custom.rs`，本 crate 最大单文件）

**加一个「单次请求 + 从 JSON 取字段」的翻译服务不需要写代码**，往 `config.json` 的
`custom_services` 追加一条 spec 即生效，也可在设置页「自定义服务」用表单添加（保存即生效、无需重启）。

支持 `auth`（`none`/`bearer`/`header`/`query`/`hash_query`）、`headers`、`body` 模板与占位符、
`extract.text`（JSON 路径）或 `extract.html_selector`（CSS 选择器）、`session`（多步会话前置请求）、
`stream_mode`/`stream_delta`（流式）、`fields`、`tls`。完整字段表见根 README。

注册顺序上有一条不变量：**自定义服务在内置服务之后注册**，这样「与内置同名」能被检出并跳过，
而不是让一条自定义 spec 悄悄顶掉内置实现。单条 spec 不合法时**只跳过并在日志里指出下标**，
不会让应用起不来。

### OCR 15 个

| 文件 | id | 备注 |
| --- | --- | --- |
| `baidu.rs` | `baidu` | 百度文字识别 |
| `baidu_accurate.rs` | `baidu_accurate` | 百度高精度版 |
| `baidu_img.rs` | `baidu_img` | 百度图片翻译（含译图） |
| `iflytek.rs` | `iflytek` | 讯飞 OCR |
| `iflytek_intsig.rs` | `iflytek_intsig` | 讯飞 × 合合 |
| `iflytek_latex.rs` | `iflytek_latex` | 讯飞公式识别 |
| `qrcode.rs` | `qrcode` | **本地**二维码解码（`rqrr`，无网络） |
| `simple_latex.rs` | `simple_latex` | **本地**简化公式识别 |
| `system.rs` | `system` | **本地**系统 OCR（分派到 `oxidict-platform::ocr`） |
| `tencent.rs` | `tencent` | 腾讯云 OCR |
| `tencent_accurate.rs` | `tencent_accurate` | 腾讯高精度版 |
| `tencent_img.rs` | `tencent_img` | 腾讯图片翻译 |
| `tesseract.rs` | `tesseract` | **本地** tesseract 命令行 |
| `volcengine.rs` | `volcengine` | 火山引擎 OCR |
| `volcengine_multi_lang.rs` | `volcengine_multi_lang` | 火山多语种版 |

默认启用列表是 `["system", "tesseract", "qrcode"]`——三个都不需要 API Key。

### TTS 3 个

| id | 备注 |
| --- | --- |
| `baidu` | 百度语音合成，需 API Key |
| `google` | 谷歌翻译的朗读端点 |
| `lingva` | Lingva 实例（默认启用） |

### 生词本 2 个

| id | 备注 |
| --- | --- |
| `anki` | AnkiConnect（本地 `8765`），需要 Anki 开着 |
| `eudic` | 欧路词典开放 API |

## HTTP 指纹

`tls` 是**实例级**配置而非全局键——同一服务的两个实例完全可以一个走官方 API、另一个指向会被 WAF
按指纹拦的网页端点。

```rust
pub const TLS_KEY: &str = "tls";
pub fn tls_field() -> ConfigField;   // 生成下拉字段，附在会被拦的服务 schema 末尾
pub const TLS_OPTIONS: &[(&str, &str)];  // native / chrome / safari
```

做成**下拉而非自由文本**：档位名必须精确匹配，写错会**静默**回退成「不模拟」，用户会以为设置没生效
却找不到原因。官方 API（带 Key 鉴权）不需要这个字段——那边按指纹拦的概率极低，白白多付一次浏览器握手。

## 测试

```bash
cargo test -p oxidict-services
```

纯函数部分（签名拼接、语言映射、relay 表的 id 稳定性、custom spec 解析）可单测；
真实翻译请求靠端到端手工验证。
