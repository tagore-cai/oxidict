//! oxidict-net：HTTP 客户端与签名工具。
//!
//! 原实现中网络请求与签名都在 JS 侧完成（`@tauri-apps/api/http` + `crypto-js`），
//! 这一层把那段逻辑完整搬到 Rust，服务实现里不再出现任何脚本代码。

pub mod backup_backend;
pub mod http;
pub mod sign;
pub mod webdav;

pub use http::{
    NetErr, check, client, client_for, get_bytes, get_bytes_for, get_json, get_text, get_text_for,
    get_value, get_value_for, get_with_headers, get_with_headers_for, post_form, post_json,
    post_json_value, post_json_value_for, post_with_headers, post_with_headers_for, rebuild_client,
    request_with_headers, request_with_headers_for, sse_payload, sse_text,
};
pub use sign::{
    AwsSignature, alibaba_escape, alibaba_nonce, aws_date, aws_now, aws_sign, derive_aws_key,
    empty_sha256, hmac_sha1_base64, hmac_sha256_base64, hmac_sha256_hex, hmac_sha256_raw,
    iso_utc_now, md5_hex, md5_hex_bytes, percent_encode, sha256_hex, tc3_sign, unix_seconds,
    uuid_v4,
};
