# oxidict-net

**网络层：HTTP 客户端、服务签名、云端备份传输。**

> 内部结构、逐文件职责、不变量清单与设计决策 → [ARCHITECTURE.md](./ARCHITECTURE.md)

原实现里网络请求与签名都在 JS 侧完成（`@tauri-apps/api/http` + `crypto-js`），由 WebView 代理到
Rust 网络层。这一层把那段逻辑完整搬到 Rust，服务实现里不再出现任何脚本代码，也省掉一次跨进程往返。

## 依赖

```
oxidict-net  →  oxidict-core
被依赖：oxidict-services、oxidict-plugin、oxidict-ui、app
```

外部依赖：`wreq`（HTTP 客户端）、`wreq-util`（指纹档案）、`hmac`/`sha1`/`sha2`/`md-5`/`hex`/`base64`、
`uuid`/`rand`（签名随机量）、`futures`（SSE 流）、`chrono`、`parking_lot`。

## 模块

| 模块 | 内容 |
| --- | --- |
| `http.rs` | wreq 客户端与指纹档位、请求便捷函数、SSE 解析、`NetErr` |
| `sign.rs` | HMAC-SHA1/256、MD5、SHA256、AWS SigV4、腾讯 TC3、阿里云转义、UUID/时间戳 |
| `webdav.rs` | WebDAV raw HTTP（`PUT`/`GET`/`MKCOL`/`DELETE`），不依赖 `reqwest_dav` 之类的封装 |
| `backup_backend.rs` | 备份后端枚举（`webdav` / `local_folder`）+ 按 `backup_type` 分派的工厂 |

## HTTP 客户端

### 为什么是 wreq 而不是 reqwest

唯一理由：**能模拟浏览器指纹**。服务一旦可以「配置一个网页端点」，端点集合就是开放的——用户能指向
`translate.google.com`、`dictionary.cambridge.org` 这类页面，而它们普遍用 WAF 按 TLS（JA3/JA4）与
HTTP/2 特征判定客户端；默认的非浏览器 ClientHello 会吃到 403 或直接被重置连接。

`wreq` 的额外收益：**保留 HTTP/1 header 的大小写**（reqwest 会小写化，部分 WAF 因此拒绝）。

代价与能力边界（BoringSSL 原生构建、根证书集是 Chrome 的、硬分叉不会自动吃到 reqwest 的安全补丁；
它只管 L1/L2，L3 会话、L4 HTML、L5 风控挑战、L6 限速都要另做）见根 README。

### 指纹档位

档位由实例配置的 `tls` 字段决定，取值 `native`（默认）/ `chrome` / `safari`：

- 档位名必须**精确匹配**——`TlsProfile::from_code` 对未知值静默回退 `Native`。宁可指纹不匹配，
  也不要因为一个拼写错误让所有请求「假装是 Safari」把排查方向带偏。设置面板因此把它渲染成下拉
  而非自由文本。
- 模拟档把 UA、Accept、client hints **一起**交给 emulation，保证「连接特征 + 头部」自洽；
  原生档保持改造前行为，显式给一个通用 UA。
- 平台串跟随当前系统：macOS 上跑却声称自己是 Windows 反而更容易被判定异常。

### 客户端缓存与失效

`CLIENTS: RwLock<Vec<(TlsProfile, Arc<Client>)>>` 按档位缓存（只有 3 个档位，线性扫描足够）。
代理配置变化后调用 `rebuild_client()` —— 指纹档位一并清掉：代理与指纹属于同一层配置，
分开失效会让「改了代理但指纹档位还用旧 client」这种不一致难以排查。

> Cookie 罐按**域名**隔离，多服务共享同一 client 不会跨域串 Cookie；而多步会话
> （先取会话再发主请求）正依赖它把 Cookie 带过去。

### 常用 API

```rust
use oxidict_net::{get_json, post_json_value, get_text, get_bytes, client_for, check};

let v: serde_json::Value = get_json(url).await?;                  // 原生档
let v = post_json_value(profile, url, &body).await?;              // 指定指纹档
let resp = check(client_for(profile).get(url).send().await?).await?;  // 非 2xx 自动转 Error::Http

// 自定义头部
let resp = get_with_headers_for(profile, url, &[("X-App", "oxidict")]).send().await?;

// SSE 流式
let mut lines = sse_text(resp);
while let Some(line) = lines.next().await {
    if let Some(payload) = sse_payload(&line?) { /* JSON 增量 */ }
}
```

`check()` 把非 2xx 转成 `Error::Http { status, body }`（body 截断到 512 字节，避免日志爆掉）。
`NetErr` trait 把 `wreq::Error` 映射成 `Error::Network`——**core 不依赖 wreq**，所以转换放在这一层，
而不是给 `oxidict_core::Error` 加 `From<wreq::Error>`。

> 代理读取走 `store_opt()` 而不是 `config()`：代理是**可选**设置，配置尚未初始化时直连即可，不该 panic
> （测试与独立工具都会走到这里）。

## 签名工具

原实现位于 JS 侧 `crypto-js`，这里用 Rust 密码学库 1:1 平移。按用途分组：

| 用途 | 函数 |
| --- | --- |
| 通用 HMAC | `hmac_sha1_base64` / `hmac_sha256_base64` / `hmac_sha256_hex` / `hmac_sha256_raw` |
| 哈希 | `sha256_hex` / `md5_hex` / `md5_hex_bytes` / `empty_sha256` |
| 编码 | `percent_encode` / `alibaba_escape` |
| 随机与时间 | `uuid_v4` / `alibaba_nonce` / `unix_seconds` / `iso_utc_now` |
| AWS SigV4 | `derive_aws_key` → `aws_sign` → `AwsSignature`（含 `aws_date` / `aws_now`） |
| 腾讯云 TC3 | `tc3_sign` |

两点必须记住：

1. **`uuid_v4()` / `unix_seconds()` 在一次请求内只能取一次**。签名模板里同时出现 `{{salt}}` 与
   `{{timestamp}}` 时，调用方必须先取一次再拼进所有位置；每次引用都重新生成会让签名对不上。
2. **`percent_encode` 与 `alibaba_escape` 不可互换**。阿里云要求 `~` 不转义、空格编码成 `%20`，
   与标准 `urlencoding` 行为不同——这是它单独存在的原因。

## 备份后端

`BackupBackend` 是 enum 派发（不是 `dyn Trait`），对齐参考实现 clippi 的 `services/backends`：

| kind | 落点 |
| --- | --- |
| `webdav`（默认） | WebDAV 服务器，程序在远端建 `oxidict-app/` 目录存 zip |
| `local_folder` | 本机目录（`backup_folder`），可指向 iCloud / OneDrive 等**由系统负责上云**的同步目录 |

```rust
let backend = BackupBackend::select(&kind, webdav_cfg, &folder)?;
backend.list().await?; backend.put(name, data).await?; backend.get(name).await?; backend.delete(name).await?;
```

**为什么用 enum 而不是 trait object**：clippi 的后端集合运行期可增（`Vec<BackendConfig>` + `dyn Trait`），
而本工程的启用后端由**单一** `backup_type` 决定、集合封闭，所以 enum 派发省掉 boxed future，
`match` 还能保证穷尽性。将来要变多后端并存，再引入 trait 不迟。

`local_folder` 不是「多一种云」，而是让用户把备份放进**系统已在同步的目录**，由系统负责上云——
这也正是原版 `backup_type` 里 `local_folder` 那一档的语义。

## 测试

本 crate 有 **19 个测试**，全部是无需网络的纯函数/缓存测试：

| 文件 | 数量 | 覆盖 |
| --- | --- | --- |
| `sign.rs` | 10 | HMAC-SHA1 已知向量、MD5/SHA256 已知向量、`percent_encode` RFC 3986 严格性、阿里云转义、AWS `derive_aws_key` 对齐官方文档向量、`aws_sign` 结构与 Host 覆盖、TC3 确定性、`unix_seconds` 合理性 |
| `webdav.rs` | 4 | PROPFIND 三种写法（小写大写无前缀）、忽略目录与非 zip、排序、空列表 |
| `backup_backend.rs` | 4 | `select` 按 `backup_type` 分派、拒绝备份名里的路径穿越、本地目录往返、`list` 忽略非备份条目 |
| `http.rs` | 1 | 客户端缓存按档位隔离且稳定 |

```bash
cargo test -p oxidict-net
```

网络的真实交互（请求/响应/流式）无法在 CI 覆盖，靠服务的端到端手工验证。
