//! 宿主能力面：`oxidict` 原生模块 + 全局对象 + legacy `utils`。
//!
//! 三种注入方式并存，插件按习惯任选：
//! 1. ESM：`import { http_request, utils } from "oxidict"`（原生模块）；
//! 2. 全局对象：`globalThis.oxidict`；
//! 3. legacy `utils`（挂在入口函数的 options 参数里传入）。
//!
//! HTTP 走 reqwest（oxidict-net），插件脚本自身不需要网络 capability；
//! 文件读取与外部程序执行都限定在插件目录内。
//!
//! 与 legacy（Tauri）宿主的差异：`utils.tauriFetch` / `utils.readBinaryFile`
//! / `utils.CryptoJS` 未实现，用到它们的插件会得到 `undefined`。

use oxidict_core::config::config;
use rquickjs::function::Async;
use rquickjs::module::{Declarations, Exports, ModuleDef};
use rquickjs::prelude::Rest;
use rquickjs::{Coerced, Ctx, Function, IntoJs, Null, Object, Result as JsResult, Value};
use std::collections::HashMap;

/// 原生模块 `oxidict`，注册进每个插件 VM 的 loader。
pub struct OxidictModule;

impl ModuleDef for OxidictModule {
    fn declare(declare: &Declarations) -> JsResult<()> {
        for name in ["http_request", "read_text_file", "run", "utils"] {
            declare.declare(name)?;
        }
        Ok(())
    }

    fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> JsResult<()> {
        // rquickjs 的 async 函数必须包在 Async newtype 里才会转成 JS Promise。
        exports.export(
            "http_request",
            Function::new(ctx.clone(), Async(http_request))?,
        )?;
        exports.export(
            "read_text_file",
            Function::new(ctx.clone(), read_text_file)?,
        )?;
        exports.export("run", Function::new(ctx.clone(), run)?)?;
        exports.export("utils", build_utils(ctx)?)?;
        Ok(())
    }
}

/// 注入全局对象：`oxidict` 与 `console`。
///
/// 老宿主是 `eval(main.js)` 的脚本语义，插件不能 `import`，
/// 全局对象是脚本插件使用宿主能力的主要途径。
pub fn inject_globals(ctx: &Ctx<'_>) -> JsResult<()> {
    let globals = ctx.globals();

    let oxidict = Object::new(ctx.clone())?;
    oxidict.set(
        "http_request",
        Function::new(ctx.clone(), Async(http_request))?,
    )?;
    oxidict.set(
        "read_text_file",
        Function::new(ctx.clone(), read_text_file)?,
    )?;
    oxidict.set("run", Function::new(ctx.clone(), run)?)?;
    oxidict.set("utils", build_utils(ctx)?)?;
    globals.set("oxidict", oxidict)?;

    // QuickJS 标准库没有 console；桥接到 log crate 方便调试插件。
    let console = Object::new(ctx.clone())?;
    console.set("log", Function::new(ctx.clone(), console_log)?)?;
    console.set("info", Function::new(ctx.clone(), console_log)?)?;
    console.set("warn", Function::new(ctx.clone(), console_warn)?)?;
    console.set("error", Function::new(ctx.clone(), console_error)?)?;
    globals.set("console", console)?;

    Ok(())
}

/// legacy `utils` 表面：挂在入口函数 options 里传入。
pub fn build_utils<'js>(ctx: &Ctx<'js>) -> JsResult<Object<'js>> {
    let utils = Object::new(ctx.clone())?;
    utils.set("cacheDir", config().app_dir().to_string_lossy().to_string())?;
    utils.set(
        "pluginDir",
        config().plugin_dir().to_string_lossy().to_string(),
    )?;
    utils.set("osType", std::env::consts::OS)?;
    utils.set("http", Function::new(ctx.clone(), Async(http_request))?)?;
    utils.set("readTextFile", Function::new(ctx.clone(), read_text_file)?)?;
    utils.set("run", Function::new(ctx.clone(), run)?)?;
    Ok(utils)
}

/// legacy `utils.http` / `oxidict.http_request` 的宿主代发实现。
/// 参数：(method, url, headers 对象, body 字符串，可省略)。
async fn http_request(
    method: String,
    url: String,
    headers: HashMap<String, String>,
    body: Option<String>,
) -> JsResult<String> {
    use oxidict_net::NetErr as _;

    let mut rb = match method.to_ascii_uppercase().as_str() {
        "POST" => oxidict_net::post_with_headers(&url, &[]),
        _ => oxidict_net::get_with_headers(&url, &[]),
    };
    for (k, v) in &headers {
        rb = rb.header(k.as_str(), v.as_str());
    }
    if let Some(body) = body {
        rb = rb.header("Content-Type", "application/json").body(body);
    }
    let resp = rb
        .send()
        .await
        .net_err()
        .map_err(|e| js_err(format!("http_request 请求失败: {e}")))?;
    let resp = oxidict_net::check(resp)
        .await
        .map_err(|e| js_err(format!("http_request 响应错误: {e}")))?;
    let text = resp
        .text()
        .await
        .net_err()
        .map_err(|e| js_err(format!("http_request 读取响应失败: {e}")))?;
    Ok(text)
}

fn read_text_file(path: String) -> JsResult<String> {
    let root = config().plugin_dir();
    let full = root.join(&path);
    if !full.starts_with(&root) {
        return Err(js_err("read_text_file: 路径越出插件目录".into()));
    }
    std::fs::read_to_string(&full).map_err(|e| js_err(format!("read_text_file 失败: {e}")))
}

fn run(program: String, args: Rest<String>) -> JsResult<String> {
    let root = config().plugin_dir();
    let full = root.join(&program);
    if !full.starts_with(&root) {
        return Err(js_err("run: 可执行文件必须位于插件目录内".into()));
    }
    let argv: Vec<String> = args.0.clone();
    std::process::Command::new(&full)
        .args(&argv)
        .output()
        .map(|out| String::from_utf8_lossy(&out.stdout).to_string())
        .map_err(|e| js_err(format!("run 失败: {e}")))
}

fn console_log(args: Rest<Coerced<String>>) {
    log::info!("{}", fmt_args(&args));
}

fn console_warn(args: Rest<Coerced<String>>) {
    log::warn!("{}", fmt_args(&args));
}

fn console_error(args: Rest<Coerced<String>>) {
    log::error!("{}", fmt_args(&args));
}

fn fmt_args(args: &Rest<Coerced<String>>) -> String {
    args.0
        .iter()
        .map(|s| s.0.clone())
        .collect::<Vec<_>>()
        .join(" ")
}

fn js_err(msg: String) -> rquickjs::Error {
    rquickjs::Error::new_from_js_message("plugin", "host", msg)
}

/// JSON → JS。用于把服务实例配置注入 options。
pub(crate) fn json_to_js<'js>(ctx: &Ctx<'js>, v: &serde_json::Value) -> JsResult<Value<'js>> {
    Ok(match v {
        serde_json::Value::Null => Null.into_js(ctx)?,
        serde_json::Value::Bool(b) => b.into_js(ctx)?,
        serde_json::Value::Number(n) => n.as_f64().unwrap_or(0.0).into_js(ctx)?,
        serde_json::Value::String(s) => s.into_js(ctx)?,
        serde_json::Value::Array(items) => {
            let arr = rquickjs::Array::new(ctx.clone())?;
            for (i, item) in items.iter().enumerate() {
                arr.set(i, json_to_js(ctx, item)?)?;
            }
            arr.into_js(ctx)?
        }
        serde_json::Value::Object(map) => {
            let obj = Object::new(ctx.clone())?;
            for (k, item) in map {
                obj.set(k.as_str(), json_to_js(ctx, item)?)?;
            }
            obj.into_js(ctx)?
        }
    })
}

/// JS → JSON。插件回传值统一转成 JSON 再出 VM 边界。
pub(crate) fn js_to_json(v: &Value<'_>) -> JsResult<serde_json::Value> {
    use rquickjs::Type;
    Ok(match v.type_of() {
        Type::Bool => serde_json::Value::Bool(v.as_bool().unwrap_or(false)),
        Type::Int | Type::Float => serde_json::Number::from_f64(v.as_number().unwrap_or(0.0))
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        Type::String => serde_json::Value::String(v.as_string().expect("string").to_string()?),
        Type::Array => {
            let arr = v.as_array().expect("array");
            let mut out = Vec::with_capacity(8);
            for item in arr.iter::<Value>() {
                out.push(js_to_json(&item?)?);
            }
            serde_json::Value::Array(out)
        }
        Type::Object => {
            let obj = v.as_object().expect("object");
            let mut map = serde_json::Map::new();
            for key in obj.own_keys::<String>(rquickjs::Filter::default()) {
                let key = key?;
                let val: Value = obj.get(key.as_str())?;
                map.insert(key, js_to_json(&val)?);
            }
            serde_json::Value::Object(map)
        }
        _ => serde_json::Value::Null,
    })
}
