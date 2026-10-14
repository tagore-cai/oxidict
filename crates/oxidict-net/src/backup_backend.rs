//! 备份后端：把「备份 zip 存到哪里」抽象成一个可切换的落点。
//!
//! 对齐参考实现 clippi 的 `services/backends`（`local_folder` / `webdav` 双后端
//! 加一个按 `backend_type` 分派的工厂）。差异只有一处：clippi 的后端集合是运行期
//! 可增的（`Vec<BackendConfig>` + `dyn Trait`），而本工程的启用后端由**单一**
//! `backup_type` 决定、集合封闭，因此用 enum 派发而不是 `dyn Trait`——省掉 boxed
//! future，`match` 还能保证穷尽性。将来若要变成多后端并存，再引入 trait 不迟。
//!
//! 本地目录后端不是「多一种云」，而是让用户把备份放进**系统已在同步的目录**
//! （iCloud Drive / OneDrive / 坚果云本地目录），由系统负责上云。这也是原版
//! `backup_type` 里 `local_folder` 那一档的语义。

use crate::webdav::{self, WebDavConfig};
use oxidict_core::{Error, Result};
use std::path::{Path, PathBuf};

/// 写入 config 的 `backup_type` 取值：WebDAV 远端。
pub const KIND_WEBDAV: &str = "webdav";
/// 写入 config 的 `backup_type` 取值：本机目录。
pub const KIND_LOCAL_FOLDER: &str = "local_folder";

/// 备份落点。
pub enum BackupBackend {
    /// WebDAV 远端目录（Nextcloud / 坚果云 / 自建等）。
    WebDav(WebDavConfig),
    /// 本机目录，通常是云盘客户端的同步目录。
    LocalFolder(PathBuf),
}

impl BackupBackend {
    /// 按 `backup_type` 选择后端。
    ///
    /// 未知取值一律退回 WebDAV（即默认值），而不是报错：一个被手改坏的
    /// `backup_type` 不应该让「备份/恢复」整块功能变成不可用。
    pub fn select(kind: &str, webdav: WebDavConfig, folder: &str) -> Result<Self> {
        match kind {
            KIND_LOCAL_FOLDER => {
                let dir = folder.trim();
                if dir.is_empty() {
                    return Err(Error::Service("本地备份目录未设置".into()));
                }
                Ok(Self::LocalFolder(PathBuf::from(dir)))
            }
            _ => Ok(Self::WebDav(webdav)),
        }
    }

    /// 后端类型标识（回写 config 用）。
    pub fn kind(&self) -> &'static str {
        match self {
            Self::WebDav(_) => KIND_WEBDAV,
            Self::LocalFolder(_) => KIND_LOCAL_FOLDER,
        }
    }

    /// 列出可用备份（按名称升序；备份名内嵌 UTC 时间戳，故末位即最新）。
    pub async fn list(&self) -> Result<Vec<String>> {
        match self {
            Self::WebDav(cfg) => webdav::list(cfg).await,
            Self::LocalFolder(dir) => list_local(dir).await,
        }
    }

    /// 上传/写入一份备份。取 `Vec<u8>` 而非 `&[u8]`：WebDAV 分支原样交给
    /// HTTP 客户端（避免一次全量拷贝），本地分支交给 `tokio::fs::write` 同理。
    pub async fn put(&self, name: &str, data: Vec<u8>) -> Result<()> {
        check_name(name)?;
        match self {
            Self::WebDav(cfg) => webdav::put(cfg, name, data).await,
            Self::LocalFolder(dir) => put_local(dir, name, data).await,
        }
    }

    /// 读取一份备份。
    pub async fn get(&self, name: &str) -> Result<Vec<u8>> {
        check_name(name)?;
        match self {
            Self::WebDav(cfg) => webdav::get(cfg, name).await,
            Self::LocalFolder(dir) => get_local(dir, name).await,
        }
    }

    /// 删除一份备份。
    pub async fn delete(&self, name: &str) -> Result<()> {
        check_name(name)?;
        match self {
            Self::WebDav(cfg) => webdav::delete(cfg, name).await,
            Self::LocalFolder(dir) => {
                let path = dir.join(name);
                tokio::fs::remove_file(&path)
                    .await
                    .map_err(|e| Error::Service(format!("删除备份失败 {}: {e}", path.display())))
            }
        }
    }
}

/// 备份名必须是「裸文件名 + `.zip`」。
///
/// 名称会直接参与路径拼接（本地后端尤其如此），必须拒绝分隔符与 `..`：
/// 否则一个被手改过的配置、或远端返回的恶意名字（`../../x.zip`）就能把读写
/// 引到目标目录之外。与插件沙箱的路径校验属同一类问题。
fn check_name(name: &str) -> Result<()> {
    let ok = !name.is_empty()
        && name.ends_with(".zip")
        && !name.contains('/')
        && !name.contains('\\')
        && !name.contains("..")
        && name != ".";
    if ok {
        Ok(())
    } else {
        Err(Error::Service(format!("备份名不合法: {name}")))
    }
}

async fn list_local(dir: &Path) -> Result<Vec<String>> {
    let mut entries = tokio::fs::read_dir(dir)
        .await
        .map_err(|e| Error::Service(format!("读取备份目录失败 {}: {e}", dir.display())))?;
    let mut out = Vec::new();
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| Error::Service(format!("遍历备份目录失败 {}: {e}", dir.display())))?
    {
        let is_file = entry
            .file_type()
            .await
            .map(|t| t.is_file())
            .unwrap_or(false);
        if !is_file {
            continue;
        }
        let path = entry.path();
        if let Some(name) = path.file_name().and_then(|n| n.to_str())
            && check_name(name).is_ok()
        {
            out.push(name.to_string());
        }
    }
    out.sort();
    Ok(out)
}

async fn put_local(dir: &Path, name: &str, data: Vec<u8>) -> Result<()> {
    tokio::fs::create_dir_all(dir)
        .await
        .map_err(|e| Error::Service(format!("创建备份目录失败 {}: {e}", dir.display())))?;
    // 先写临时文件再 rename：写到一半被中断时不会留下半个 zip。这点在
    // 「目录由云盘客户端同步」的场景尤其重要——半成品会被后台传上去。
    let tmp = dir.join(format!("{name}.part"));
    tokio::fs::write(&tmp, data)
        .await
        .map_err(|e| Error::Service(format!("写入备份失败 {}: {e}", tmp.display())))?;
    let final_path = dir.join(name);
    tokio::fs::rename(&tmp, &final_path)
        .await
        .map_err(|e| Error::Service(format!("落盘备份失败 {}: {e}", final_path.display())))
}

async fn get_local(dir: &Path, name: &str) -> Result<Vec<u8>> {
    let path = dir.join(name);
    tokio::fs::read(&path)
        .await
        .map_err(|e| Error::Service(format!("读取备份失败 {}: {e}", path.display())))
}

#[cfg(test)]
mod tests {
    // 测试中 unwrap/expect 直观且失败即测试失败，豁免。
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("oxidict-backend-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn webdav_cfg() -> WebDavConfig {
        WebDavConfig {
            url: "https://example.invalid/dav".into(),
            username: "u".into(),
            password: "p".into(),
        }
    }

    /// `backup_type` 为 `local_folder` 时选中目录后端；未设目录要明确报错，
    /// 而不是悄悄退化成 WebDAV（那会让人以为备份去了本地、实际传到了远端）。
    #[test]
    fn select_dispatches_on_backup_type() {
        let b = BackupBackend::select(KIND_LOCAL_FOLDER, webdav_cfg(), "/tmp/bak").unwrap();
        assert_eq!(b.kind(), KIND_LOCAL_FOLDER);
        assert!(matches!(b, BackupBackend::LocalFolder(_)));

        assert!(
            BackupBackend::select(KIND_LOCAL_FOLDER, webdav_cfg(), "   ").is_err(),
            "空目录必须报错"
        );

        for kind in [KIND_WEBDAV, "unknown-value", ""] {
            let b = BackupBackend::select(kind, webdav_cfg(), "/tmp/bak").unwrap();
            assert_eq!(b.kind(), KIND_WEBDAV, "kind={kind} 应回退 WebDAV");
        }
    }

    /// 备份名参与路径拼接，必须挡住穿越与绝对路径。
    #[test]
    fn rejects_path_traversal_in_backup_name() {
        for bad in [
            "../../etc/passwd.zip",
            "../a.zip",
            "/etc/a.zip",
            "sub/a.zip",
            "sub\\a.zip",
            "a.txt",
            "",
        ] {
            assert!(check_name(bad).is_err(), "应拒绝: {bad}");
        }
        assert!(check_name("oxidict-backup-2026-09-13T120000Z.zip").is_ok());
    }

    /// 本地目录后端的完整往返：写入 → 列出 → 读回 → 删除。
    #[test]
    fn local_folder_round_trip() {
        let dir = temp_dir("roundtrip");
        let backend = BackupBackend::LocalFolder(dir.clone());
        let name = "oxidict-backup-2026-09-13T120000Z.zip";
        let payload: Vec<u8> = b"PK\x03\x04fake".to_vec();

        oxidict_core::runtime::handle().block_on(async {
            // 契约：目录不存在时 `list` **报错**而不是返回空。目录不存在通常意味着
            // 路径写错了，静默返回空会被误读成「备份都没了」。`put` 则允许创建目录。
            assert!(
                backend.list().await.is_err(),
                "目录不存在时应报错，而非静默返回空"
            );

            backend.put(name, payload.clone()).await.unwrap();
            assert_eq!(backend.list().await.unwrap(), vec![name.to_string()]);
            assert_eq!(backend.get(name).await.unwrap(), payload);

            // 临时文件不能残留（否则会被当成待同步内容或被误列为备份）。
            let leftovers: Vec<_> = std::fs::read_dir(&dir)
                .unwrap()
                .flatten()
                .filter(|e| e.file_name().to_string_lossy().ends_with(".part"))
                .collect();
            assert!(leftovers.is_empty(), "残留 .part 文件: {leftovers:?}");

            backend.delete(name).await.unwrap();
            assert!(backend.list().await.unwrap().is_empty());
        });

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 目录里的非 zip、子目录、以及 .part 半成品都不应被列为可用备份。
    #[test]
    fn list_ignores_non_backup_entries() {
        let dir = temp_dir("filter");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("notes.txt"), b"x").unwrap();
        std::fs::write(dir.join("half.zip.part"), b"x").unwrap();
        std::fs::create_dir_all(dir.join("sub.zip")).unwrap();
        std::fs::write(dir.join("b.zip"), b"x").unwrap();
        std::fs::write(dir.join("a.zip"), b"x").unwrap();

        let backend = BackupBackend::LocalFolder(dir.clone());
        let names =
            oxidict_core::runtime::handle().block_on(async { backend.list().await.unwrap() });
        // 目录名以 .zip 结尾也应排除（file_type 判定为目录）。
        assert_eq!(names, vec!["a.zip".to_string(), "b.zip".to_string()]);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
