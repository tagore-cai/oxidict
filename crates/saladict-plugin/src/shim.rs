//! 把老的 `.potext` 插件改造成 gpui-shell 插件。
//!
//! 老插件的 `main.js` 是一个脚本，结尾处定义 `function translate(text, from, to, options)`，
//! 宿主用 `eval` 取出这个函数后直接调用。gpui-shell 是 ESM，没有 eval，
//! 所以安装时做一次机械改写：原文件改名 `legacy.js`，再生成一个 ESM 包装层，
//! 把 `legacy.js` 里的函数导出，并在 `worker.js` 里跑作业循环。

use crate::Result;
use std::path::Path;

/// 老插件的四大类，决定导出哪个函数。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginKind {
    Translate,
    Recognize,
    Tts,
    Collection,
}

impl PluginKind {
    pub fn as_str(self) -> &'static str {
        match self {
            PluginKind::Translate => "translate",
            PluginKind::Recognize => "recognize",
            PluginKind::Tts => "tts",
            PluginKind::Collection => "collection",
        }
    }

    pub fn dir_name(self) -> &'static str {
        match self {
            PluginKind::Translate => "translate",
            PluginKind::Recognize => "recognize",
            PluginKind::Tts => "tts",
            PluginKind::Collection => "collection",
        }
    }

    /// 该类型插件必须导出的函数名。
    pub fn entry_function(self) -> &'static str {
        match self {
            PluginKind::Translate => "translate",
            PluginKind::Recognize => "recognize",
            PluginKind::Tts => "tts",
            PluginKind::Collection => "collection",
        }
    }
}

/// 生成 `legacy.js`：把原 `main.js` 的内容转成 ESM 模块并导出目标函数。
pub fn generate_legacy_module(dir: &Path, kind: PluginKind) -> Result<()> {
    let main = dir.join("main.js");
    let source = std::fs::read_to_string(&main)?;
    let func = kind.entry_function();

    let mut out = String::with_capacity(source.len() + 256);
    out.push_str("// 由 saladict 安装器生成：把 .potext 的 main.js 转成 ESM。\n");
    out.push_str("import { utils as __get_utils } from \"saladict\";\nconst utils = __get_utils();\n\n");
    out.push_str(source.trim_end());
    out.push_str("\n\n");
    out.push_str(&format!(
        "export {{ {} }};\nexport default {};\n",
        func, func
    ));

    std::fs::write(dir.join("legacy.js"), out)?;
    let _ = std::fs::remove_file(&main);
    Ok(())
}

/// 生成 `worker.js`：插件入口，一个 View，启动后进入作业循环。
pub fn generate_worker(dir: &Path, plugin_id: &str, kind: PluginKind) -> Result<()> {
    let func = kind.entry_function();
    let worker = format!(
        r#"// 由 saladict 安装器生成。
// gpui-shell 的插件入口必须是 View，而 .potext 插件是纯函数，
// 因此这里把控制权反转：宿主把作业放进队列，本循环取出来转交给 legacy 实现。
import {{ View, div }} from "gpui-kit";
import {{ take_job, submit_result }} from "saladict";
import {{ {func} }} from "./legacy.js";

const PLUGIN_ID = {plugin_id:?};

export default class SaladictWorker extends View {{
  init() {{
    this.running = true;
    this.pump();
  }}

  async pump() {{
    while (this.running) {{
      const job = await take_job(PLUGIN_ID);
      if (job === null || job === undefined) break;
      try {{
        const options = {{
          config: job.config || {{}},
          detect: job.from,
          setResult: () => {{}},
          utils,
        }};
        const out = await {func}(
          job.kind === "recognize" ? job.image : job.text,
          job.from,
          job.to,
          options,
        );
        submit_result(PLUGIN_ID, job.id, true, out === undefined ? null : out);
      }} catch (error) {{
        submit_result(PLUGIN_ID, job.id, false, String(error));
      }}
    }}
  }}

  render(cx) {{
    return div();
  }}
}}
"#,
        func = func,
        plugin_id = plugin_id,
    );
    std::fs::write(dir.join("worker.js"), worker)?;
    Ok(())
}

/// 生成 `gpui-shell.json` 清单。
pub fn generate_manifest(dir: &Path, plugin_id: &str, name: &str, version: &str) -> Result<()> {
    let manifest = serde_json::json!({
        "id": plugin_id,
        "name": name,
        "version": version,
        "entry": "worker.js",
        "capabilities": {
            "network": true,
            "store": true,
        }
    });
    std::fs::write(
        dir.join("gpui-shell.json"),
        serde_json::to_string_pretty(&manifest)?,
    )?;
    Ok(())
}

/// 一次性生成全部三件套。
pub fn convert_potext(dir: &Path, plugin_id: &str, name: &str, version: &str, kind: PluginKind) -> Result<()> {
    generate_legacy_module(dir, kind)?;
    generate_worker(dir, plugin_id, kind)?;
    generate_manifest(dir, plugin_id, name, version)?;
    Ok(())
}
