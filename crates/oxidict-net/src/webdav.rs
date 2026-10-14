//! WebDAV 云端备份（raw HTTP 实现，不依赖 reqwest_dav 之类的封装）。

use crate::{NetErr, check, client};
use oxidict_core::{Error, Result};

pub struct WebDavConfig {
    pub url: String,
    pub username: String,
    pub password: String,
}

fn dav_url(base: &str, path: &str) -> String {
    let base = base.trim_end_matches('/');
    format!("{base}/{}", path.trim_start_matches('/'))
}

fn mkcol() -> wreq::Method {
    // 不变量："MKCOL" 是合法的 HTTP method 字节序列，from_bytes 不可能失败。
    wreq::Method::from_bytes(b"MKCOL").expect("静态合法的 HTTP method")
}

fn propfind() -> wreq::Method {
    // 不变量："PROPFIND" 是合法的 HTTP method 字节序列。
    wreq::Method::from_bytes(b"PROPFIND").expect("静态合法的 HTTP method")
}

async fn ensure_dir(cfg: &WebDavConfig) {
    let url = dav_url(&cfg.url, "oxidict-app");
    let _ = client()
        .request(mkcol(), &url)
        .basic_auth(&cfg.username, Some(&cfg.password))
        .send()
        .await;
}

/// 列出远端备份目录下的 .zip 文件名。
pub async fn list(cfg: &WebDavConfig) -> Result<Vec<String>> {
    ensure_dir(cfg).await;
    let url = dav_url(&cfg.url, "oxidict-app/");
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
///
/// 必须大小写不敏感、且容忍命名空间前缀：Nextcloud / SabreDAV 返回小写
/// `<d:href>`，Apache mod_dav 返回大写 `<D:href>`，也有服务器直接给 `<href>`。
/// 只认某一种写法会让云端备份列表在多数服务器上恒为空，恢复功能直接失效。
pub fn parse_zip_names(body: &str) -> Vec<String> {
    let mut files = Vec::new();
    let mut cursor = 0;
    while let Some(lt) = body[cursor..].find('<') {
        let tag_start = cursor + lt;
        let Some(gt_rel) = body[tag_start..].find('>') else {
            break;
        };
        let tag_end = tag_start + gt_rel;
        let inner = &body[tag_start + 1..tag_end];
        cursor = tag_end + 1;

        // 闭合标签（`</d:href>`）、处理指令（`<?xml ...?>`）、声明一律跳过。
        if inner.starts_with('/') || inner.starts_with('?') || inner.starts_with('!') {
            continue;
        }
        // 带命名空间前缀时取最后一段：`D:href` / `d:href` / `href` 都归一到 `href`。
        let local = inner.rsplit(':').next().unwrap_or(inner).trim();
        if !local.eq_ignore_ascii_case("href") {
            continue;
        }

        // 元素值 = 开标签之后、下一个 '<' 之前。
        let value_end = body[cursor..]
            .find('<')
            .map(|r| cursor + r)
            .unwrap_or(body.len());
        let raw = body[cursor..value_end].trim();
        // href 可能是绝对 URL（`http://host/dav/oxidict-app/x.zip`）或带目录的
        // 路径，只取最后一段文件名。
        let name = raw.rsplit('/').next().unwrap_or("").trim();
        if !name.is_empty() && name.ends_with(".zip") && !files.iter().any(|f| f == name) {
            files.push(name.to_string());
        }
    }
    // 备份名形如 `oxidict-backup-<UTC 时间戳>.zip`，字典序即时间序，
    // 排序后调用方取 `.last()` 才能可靠地拿到「最近一次」。
    files.sort();
    files
}

/// 上传备份。
pub async fn put(cfg: &WebDavConfig, name: &str, data: Vec<u8>) -> Result<()> {
    ensure_dir(cfg).await;
    let url = dav_url(&cfg.url, &format!("oxidict-app/{name}"));
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
    let url = dav_url(&cfg.url, &format!("oxidict-app/{name}"));
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
    let url = dav_url(&cfg.url, &format!("oxidict-app/{name}"));
    let resp = client()
        .delete(&url)
        .basic_auth(&cfg.username, Some(&cfg.password))
        .send()
        .await
        .net_err()?;
    check(resp).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    // 测试中 unwrap 直观且失败即测试失败，豁免。
    #![allow(clippy::unwrap_used)]

    use super::parse_zip_names;

    /// 真实服务器对命名空间前缀的大小写并不统一，解析必须三种写法都认，
    /// 否则「云端备份列表」在多数 WebDAV 上恒为空、恢复功能静默失效。
    #[test]
    fn parses_lowercase_uppercase_and_bare_href() {
        let cases = [
            r#"<?xml version="1.0"?><d:multistatus xmlns:d="DAV:"><d:response><d:href>/dav/oxidict-app/a.zip</d:href></d:response></d:multistatus>"#,
            r#"<D:multistatus><D:response><D:href>/oxidict-app/a.zip</D:href></D:response></D:multistatus>"#,
            r#"<multistatus><response><href>http://host/dav/oxidict-app/a.zip</href></response></multistatus>"#,
        ];
        for body in cases {
            assert_eq!(parse_zip_names(body), vec!["a.zip"], "body={body}");
        }
    }

    /// 目录自身的 href（以 `/` 结尾）与子目录路径不能被误当备份文件。
    #[test]
    fn ignores_directories_and_non_zip() {
        let body = r#"<d:multistatus>
            <d:response><d:href>/dav/oxidict-app/</d:href></d:response>
            <d:response><d:href>/dav/oxidict-app/subdir/</d:href></d:response>
            <d:response><d:href>/dav/oxidict-app/notes.txt</d:href></d:response>
            <d:response><d:href>/dav/oxidict-app/b.zip</d:href></d:response>
        </d:multistatus>"#;
        assert_eq!(parse_zip_names(body), vec!["b.zip"]);
    }

    /// 结果按字典序排序：备份名内嵌 UTC 时间戳，调用方取 `.last()` 才是
    /// 「最近一次」，不能依赖服务器返回顺序。
    #[test]
    fn sorts_by_name_so_last_is_latest() {
        let body = r#"<d:multistatus>
            <d:response><d:href>/x/oxidict-backup-2026-09-13T120000Z.zip</d:href></d:response>
            <d:response><d:href>/x/oxidict-backup-2026-09-11T080000Z.zip</d:href></d:response>
            <d:response><d:href>/x/oxidict-backup-2026-09-12T090000Z.zip</d:href></d:response>
        </d:multistatus>"#;
        let names = parse_zip_names(body);
        assert_eq!(names.len(), 3);
        assert_eq!(
            names.last().unwrap(),
            "oxidict-backup-2026-09-13T120000Z.zip"
        );
    }

    /// 无备份文件时返回空列表（调用方据此提示「云端没有备份文件」）。
    #[test]
    fn empty_when_no_backups() {
        let body = r#"<d:multistatus><d:response><d:href>/dav/oxidict-app/</d:href></d:response></d:multistatus>"#;
        assert!(parse_zip_names(body).is_empty());
    }
}
