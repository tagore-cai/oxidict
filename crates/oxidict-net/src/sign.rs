//! 各翻译/OCR 服务所需的签名算法。
//!
//! 原实现位于 JS 侧 `crypto-js`，这里用 Rust 密码学库 1:1 平移。

use base64::Engine;
use hmac::{Hmac, KeyInit, Mac};
use md5::{Digest, Md5};
use sha1::Sha1;
use sha2::Sha256;

type HmacSha1 = Hmac<Sha1>;
type HmacSha256 = Hmac<Sha256>;

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

pub fn hmac_sha1_base64(key: &[u8], msg: &str) -> String {
    let mut mac = HmacSha1::new_from_slice(key).expect("HMAC-SHA1 accepts any key length");
    mac.update(msg.as_bytes());
    b64(&mac.finalize().into_bytes())
}

pub fn hmac_sha256_base64(key: &[u8], msg: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC-SHA256 accepts any key length");
    mac.update(msg.as_bytes());
    b64(&mac.finalize().into_bytes())
}

pub fn hmac_sha256_hex(key: &[u8], msg: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC-SHA256 accepts any key length");
    mac.update(msg.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

pub fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

/// 空内容的 SHA256，AWS 签名里作为 `x-content-sha256` 的默认值。
pub fn empty_sha256() -> String {
    sha256_hex(b"")
}

/// 对二进制数据取 MD5（百度图片翻译用它校验图片内容）。
pub fn md5_hex_bytes(data: &[u8]) -> String {
    let mut hasher = Md5::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

pub fn md5_hex(s: &str) -> String {
    let mut hasher = Md5::new();
    hasher.update(s.as_bytes());
    hex::encode(hasher.finalize())
}

/// 当前 Unix 时间戳（秒）。
///
/// 部分签名协议需要它（如有道 v3 的 `curtime`）。故意不做成"可注入时钟"：
/// 需要确定性的测试应该直接测签名明文模板，而不是把时钟抽象化。
pub fn unix_seconds() -> i64 {
    chrono::Utc::now().timestamp()
}

pub fn uuid_v4() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// 阿里云风格的随机 nonce：`(100000..199999) * 1000`。
pub fn alibaba_nonce() -> u64 {
    // rand 0.10 移除了 thread_rng()/gen_range()：改用自由函数 random_range，
    // 内部仍走线程本地 RNG，无需显式构造。
    rand::random_range(100_000..200_000u64) * 1000
}

/// UTC 时间戳，格式 `2024-01-01T12:00:00Z`。
pub fn iso_utc_now() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

/// AWS SigV4 要求的日期格式 `20240101T120000Z`。
pub fn aws_now() -> String {
    chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string()
}

pub fn aws_date() -> String {
    chrono::Utc::now().format("%Y%m%d").to_string()
}

/// RFC3986 严格百分号编码（仅 A-Za-z0-9-_.~ 不转义）。
///
/// AWS SigV4 要求这种形式；阿里云 JS 原实现里 `encodeURIComponent` 会放过
/// `!'()*`，但随后的二次转义链把 `%21` 等替换为 `%25xx`，最终结果与本实现的
/// 输出一致（见 `alibaba_escape` 与其单测）。
pub fn percent_encode(s: &str) -> String {
    const UNRESERVED: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_.~";
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if UNRESERVED.contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

/// 阿里云 POP 签名里对 stringToSign 的二次转义。
/// JS 侧用 `encodeURIComponent` 后再把 `!'()*+,` 替换成 `%25xx`，这里等价实现。
pub fn alibaba_escape(s: &str) -> String {
    percent_encode(s)
        .replace("%21", "%2521")
        .replace("%27", "%2527")
        .replace("%28", "%2528")
        .replace("%29", "%2529")
        .replace("%2A", "%252A")
        .replace("%2B", "%252B")
        .replace("%2C", "%252C")
}

/// AWS SigV4 的派生密钥：`kSigning = HMAC(HMAC(HMAC(HMAC("AWS4"+secret, date), region), service), "aws4_request")`。
///
/// 首环必须是 `"AWS4" + secret` 作为密钥、`date` 作为消息。早期实现写成
/// `HMAC(secret + date, date)`，既不是 AWS（缺 `AWS4` 前缀），也不是火山引擎
/// （首环应为 `HMAC(secret, date)`，见 `oxidict-services` 的 volcengine 实现），
/// 任何按此签名的请求都会被服务端判为签名不匹配。
pub fn derive_aws_key(secret: &str, date: &str, region: &str, service: &str) -> Vec<u8> {
    let k_date = hmac_sha256_raw(format!("AWS4{}", secret).as_bytes(), date.as_bytes());
    let k_region = hmac_sha256_raw(&k_date, region.as_bytes());
    let k_service = hmac_sha256_raw(&k_region, service.as_bytes());
    hmac_sha256_raw(&k_service, b"aws4_request")
}

pub fn hmac_sha256_raw(key: &[u8], msg: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC-SHA256 accepts any key length");
    mac.update(msg);
    mac.finalize().into_bytes().to_vec()
}

/// AWS SigV4 完整签名流程，返回 `Authorization` 头所需的各分量。
///
/// 注意与火山引擎的区别：火山系（`oxidict-services` 的 volcengine 翻译/OCR）
/// 用的是同族但**不等价**的变体（首环无 `AWS4` 前缀、尾缀为 `request`），
/// 因此它们各自就地实现，不复用本函数。
pub struct AwsSignature {
    pub credential: String,
    pub signed_headers: String,
    pub signature: String,
    pub x_date: String,
}

/// 参数个数由 AWS SigV4 算法本身决定（方法/主机/路径/查询/头/载荷/密钥对/
/// 区域/服务），拆成 builder 反而会把调用方搞复杂，这里保留平直签名。
#[allow(clippy::too_many_arguments)]
pub fn aws_sign(
    method: &str,
    host: &str,
    path: &str,
    query: &str,
    headers: &[(&str, &str)],
    payload: &[u8],
    access_key: &str,
    secret_key: &str,
    region: &str,
    service: &str,
) -> AwsSignature {
    let x_date = aws_now();
    let short_date = aws_date();

    // SigV4 要求 `host` 必须参与签名。早期实现把 host 参数整个丢弃
    // （`let _ = host;`），只在调用方恰好自带 host 头时才算对；这里在缺失时
    // 自动补上，保证签名集合始终包含 host。
    let mut signed: Vec<(&str, &str)> = headers.to_vec();
    if !signed.iter().any(|(k, _)| k.eq_ignore_ascii_case("host")) {
        signed.push(("host", host));
    }
    signed.push(("x-date", x_date.as_str()));
    signed.sort_by_key(|(k, _)| k.to_ascii_lowercase());
    let signed_headers = signed
        .iter()
        .map(|(k, _)| k.to_ascii_lowercase())
        .collect::<Vec<_>>()
        .join(";");

    let canonical_headers = signed
        .iter()
        .map(|(k, v)| format!("{}:{}\n", k.to_ascii_lowercase(), v.trim()))
        .collect::<String>();

    let payload_hash = sha256_hex(payload);

    let canonical_request = format!(
        "{}\n{}\n{}\n{}\n{}\n{}",
        method, path, query, canonical_headers, signed_headers, payload_hash
    );

    // 凭证范围尾缀固定为 `aws4_request`，算法标识固定为 `AWS4-HMAC-SHA256`；
    // 写成 `request` / `HMAC-SHA256` 会让服务端判为签名不匹配。
    let scope = format!("{}/{}/{}/aws4_request", short_date, region, service);
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{}\n{}\n{}",
        x_date,
        scope,
        sha256_hex(canonical_request.as_bytes())
    );

    let signing_key = derive_aws_key(secret_key, &short_date, region, service);
    let signature = hex::encode(hmac_sha256_raw(&signing_key, string_to_sign.as_bytes()));

    AwsSignature {
        credential: format!("{}/{}", access_key, scope),
        signed_headers,
        signature,
        x_date,
    }
}

/// 腾讯翻译君的 TC3 签名（HMAC-SHA256 链式派生）。
pub fn tc3_sign(
    secret_key: &str,
    date: &str,
    service: &str,
    canonical_request: &str,
    string_to_sign_prefix: &str,
) -> String {
    let secret_date = hmac_sha256_raw(format!("TC3{}", secret_key).as_bytes(), date.as_bytes());
    let secret_service = hmac_sha256_raw(&secret_date, service.as_bytes());
    let secret_signing = hmac_sha256_raw(&secret_service, b"tc3_request");
    let string_to_sign = format!(
        "{}\n{}",
        string_to_sign_prefix,
        sha256_hex(canonical_request.as_bytes())
    );
    hex::encode(hmac_sha256_raw(&secret_signing, string_to_sign.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 与 JS `crypto-js` 输出对齐的已知向量，防止签名平移走样。
    #[test]
    fn hmac_sha1_base64_matches_known_vector() {
        // RFC 2202 Test Case 1: key = 0x0b*20, data = "Hi There"
        let key = [0x0bu8; 20];
        // 经 Python hmac/hashlib 权威验证的值（RFC 2202 TC1）。
        assert_eq!(
            hmac_sha1_base64(&key, "Hi There"),
            "thcxhlUFcmTii8C2+zeMjvFGvgA="
        );
    }

    #[test]
    fn percent_encode_is_rfc3986_strict() {
        // 空格必须是 %20 而不是 +；!'()* 按严格模式编码（AWS 要求）。
        assert_eq!(percent_encode("a b"), "a%20b");
        assert_eq!(percent_encode("a+b"), "a%2Bb");
        assert_eq!(percent_encode("!'()*"), "%21%27%28%29%2A");
        assert_eq!(percent_encode("你好"), "%E4%BD%A0%E5%A5%BD");
    }

    #[test]
    fn alibaba_escape_double_escapes_specials() {
        // 原 JS：encodeURIComponent 后再替换 !'()*+, 为 %25xx。
        assert_eq!(alibaba_escape("a b!"), "a%20b%2521");
        assert_eq!(alibaba_escape("a+b"), "a%252Bb");
    }

    /// 时间戳只校验量级，不钉死具体值——那是时钟读数，钉死必然脆弱。
    #[test]
    fn unix_seconds_is_a_plausible_epoch() {
        let now = unix_seconds();
        assert!(now > 1_600_000_000, "过小，可能不是秒: {now}");
        assert!(now < 4_102_444_800, "过大，可能不是秒: {now}");
    }

    #[test]
    fn md5_matches_known_vector() {
        assert_eq!(md5_hex("hello"), "5d41402abc4b2a76b9719d911017c592");
    }

    #[test]
    fn sha256_empty_matches() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn aws_sign_produces_expected_structure() {
        // 结构校验：signature 为 64 位 hex，signed_headers 按字母序。
        let sig = aws_sign(
            "POST",
            "open.volcengineapi.com",
            "/",
            "Action=TranslateText&Version=2020-06-01",
            &[
                ("content-type", "application/json"),
                ("host", "open.volcengineapi.com"),
                ("x-content-sha256", "abc"),
                ("x-date", "20240101T120000Z"),
            ],
            b"{}",
            "ak",
            "sk",
            "cn-north-1",
            "translate",
        );
        assert_eq!(sig.signature.len(), 64);
        assert!(sig.signed_headers.starts_with("content-type;"));
        assert!(sig.credential.starts_with("ak/"));
        // 凭证范围尾缀与算法标识必须符合 SigV4 规范。
        assert!(sig.credential.ends_with("/aws4_request"));
    }

    /// AWS SigV4 官方测试向量（《Examples of the complete Version 4 signing
    /// process》）：派生密钥必须与文档给出的值逐字节一致。
    ///
    /// 该向量能同时锁住首环的 `"AWS4"` 前缀与尾环的 `"aws4_request"`——早期
    /// 实现写成 `HMAC(secret + date, date)` 并配 `b"request"`，本条会失败。
    #[test]
    fn derive_aws_key_matches_aws_documented_vector() {
        let key = derive_aws_key(
            "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY",
            "20150830",
            "us-east-1",
            "iam",
        );
        assert_eq!(
            hex::encode(key),
            "c4afb1cc5771d871763a393e44b703571b55cc28424d1a5e86da6ed3c154a4b9"
        );
    }

    /// host 未由调用方显式提供时，必须自动加入签名集合。
    #[test]
    fn aws_sign_always_covers_host() {
        let sig = aws_sign(
            "GET",
            "example.amazonaws.com",
            "/",
            "",
            &[],
            b"",
            "ak",
            "sk",
            "us-east-1",
            "service",
        );
        assert!(
            sig.signed_headers.split(';').any(|h| h == "host"),
            "signed_headers={}",
            sig.signed_headers
        );
        assert!(sig.signed_headers.ends_with("host;x-date"));
    }

    #[test]
    fn tc3_sign_is_deterministic() {
        let a = tc3_sign(
            "sk",
            "2024-01-01",
            "tmt",
            "canonical",
            "TC3-HMAC-SHA256\n1\nscope",
        );
        let b = tc3_sign(
            "sk",
            "2024-01-01",
            "tmt",
            "canonical",
            "TC3-HMAC-SHA256\n1\nscope",
        );
        assert_eq!(a, b);
        assert_eq!(a.len(), 64);
    }
}
