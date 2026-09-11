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

/// 火山引擎 / 字节系使用的 HMAC-SHA256 签名：`HMAC(HMAC(kDate, region), kService)` 派生密钥。
pub fn derive_aws_key(secret: &str, date: &str, region: &str, service: &str) -> Vec<u8> {
    let k_date = hmac_sha256_raw(format!("{}{}", secret, date).as_bytes(), date.as_bytes());
    let k_region = hmac_sha256_raw(&k_date, region.as_bytes());
    let k_service = hmac_sha256_raw(&k_region, service.as_bytes());
    hmac_sha256_raw(&k_service, b"request")
}

pub fn hmac_sha256_raw(key: &[u8], msg: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC-SHA256 accepts any key length");
    mac.update(msg);
    mac.finalize().into_bytes().to_vec()
}

/// 火山引擎 OCR 的完整签名流程，返回 `Authorization` 头所需的五个分量。
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

    let mut signed: Vec<(&str, &str)> = headers.to_vec();
    signed.push(("x-date", x_date.as_str()));
    signed.sort_by(|a, b| a.0.cmp(b.0));
    let signed_headers = signed.iter().map(|(k, _)| *k).collect::<Vec<_>>().join(";");

    let canonical_headers = signed
        .iter()
        .map(|(k, v)| format!("{}:{}\n", k, v.trim()))
        .collect::<String>();

    let payload_hash = sha256_hex(payload);

    let canonical_request = format!(
        "{}\n{}\n{}\n{}\n{}\n{}",
        method, path, query, canonical_headers, signed_headers, payload_hash
    );

    let scope = format!("{}/{}/{}/request", short_date, region, service);
    let string_to_sign = format!(
        "HMAC-SHA256\n{}\n{}\n{}",
        x_date,
        scope,
        sha256_hex(canonical_request.as_bytes())
    );

    let signing_key = derive_aws_key(secret_key, &short_date, region, service);
    let signature = hex::encode(hmac_sha256_raw(&signing_key, string_to_sign.as_bytes()));
    let _ = host;

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
