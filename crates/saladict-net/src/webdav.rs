//! WebDAV 云端备份（raw reqwest 实现，不依赖 reqwest_dav）。

use crate::{check, client, NetErr};
use saladict_core::{Error, Result};

pub struct WebDavConfig {
    pub url: String,
    pub username: String,
    pub password: String,
}

fn dav_url(base: &str, path: &str) -> String {
    let base = base.trim_end_matches('/');
    format!("{base}/{}", path.trim_start_matches('/'))
}

fn mkcol() -> reqwest::Method {
    // 不变量："MKCOL" 是合法的 HTTP method 字节序列，from_bytes 不可能失败。
    reqwest::Method::from_bytes(b"MKCOL").expect("静态合法的 HTTP method")
}

fn propfind() -> reqwest::Method {
    // 不变量："PROPFIND" 是合法的 HTTP method 字节序列。
    reqwest::Method::from_bytes(b"PROPFIND").expect("静态合法的 HTTP method")
}

async fn ensure_dir(cfg: &WebDavConfig) {
    let url = dav_url(&cfg.url, "saladict-app");
    let _ = client()
        .request(mkcol(), &url)
        .basic_auth(&cfg.username, Some(&cfg.password))
        .send()
        .await;
}

/// 列出远端备份目录下的 .zip 文件名。
pub async fn list(cfg: &WebDavConfig) -> Result<Vec<String>> {
    ensure_dir(cfg).await;
    let url = dav_url(&cfg.url, "saladict-app/");
    let resp = client()
        .request(propfind(), &url)
        .header("Depth", "1")
        .basic_auth(&cfg.username, Some(&cfg.password))
        .send()
        .await
        .net_err()?;
    let resp = check(resp).await?;
    let body = resp.text().await.net_err()?;
    Ok(parse_zip_names(&body))
}

/// 从 PROPFIND XML 提取 .zip 文件名（简单字符串解析，不引入 xml crate）。
pub fn parse_zip_names(body: &str) -> Vec<String> {
    let mut files = Vec::new();
    for segment in body.split('<') {
        if let Some(rest) = segment.strip_prefix("D:href>") {
            let name = rest
                .trim_end_matches("</D:href")
                .trim_start_matches("/saladict-app/");
            if !name.is_empty() && name.ends_with(".zip") {
                files.push(name.to_string());
            }
        }
    }
    files
}

/// 上传备份。
pub async fn put(cfg: &WebDavConfig, name: &str, data: Vec<u8>) -> Result<()> {
    ensure_dir(cfg).await;
    let url = dav_url(&cfg.url, &format!("saladict-app/{name}"));
    let resp = client()
        .put(&url)
        .basic_auth(&cfg.username, Some(&cfg.password))
        .body(data)
        .send()
        .await
        .map_err(|e| Error::Network(format!("WebDAV 上传失败: {e}")))?;
    check(resp).await?;
    Ok(())
}

/// 下载备份。
pub async fn get(cfg: &WebDavConfig, name: &str) -> Result<Vec<u8>> {
    let url = dav_url(&cfg.url, &format!("saladict-app/{name}"));
    let resp = client()
        .get(&url)
        .basic_auth(&cfg.username, Some(&cfg.password))
        .send()
        .await
        .net_err()?;
    let resp = check(resp).await?;
    let data = resp.bytes().await.net_err()?;
    Ok(data.to_vec())
}

/// 删除远端备份。
pub async fn delete(cfg: &WebDavConfig, name: &str) -> Result<()> {
    let url = dav_url(&cfg.url, &format!("saladict-app/{name}"));
    let resp = client()
        .delete(&url)
        .basic_auth(&cfg.username, Some(&cfg.password))
        .send()
        .await
        .net_err()?;
    check(resp).await?;
    Ok(())
}
