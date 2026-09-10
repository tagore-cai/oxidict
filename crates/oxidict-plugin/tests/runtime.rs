//! 插件运行时端到端测试：真实加载 `main.js` → 调用入口函数 → 校验回传。
//!
//! 覆盖：同步返回、async 返回（Promise）、对象（词典结构）回传、
//! JS 异常的 message 透传、options（config/detect/utils）注入、
//! 全局 `oxidict` 对象存在性。

use oxidict_core::config::ConfigStore;
use oxidict_plugin::runtime::{self, CallRequest};

const MAIN_JS: &str = r#"
function translate(text, from, to, options) {
  if (text === "async") {
    return Promise.resolve("ASYNC:" + from + ":" + to);
  }
  if (text === "dict") {
    return { explanations: ["解释一", "解释二"] };
  }
  if (text === "err") {
    throw new Error("boom");
  }
  if (text === "config") {
    return "apiKey=" + options.config.apiKey + ",detect=" + options.detect;
  }
  if (text === "utils") {
    return typeof options.utils === "object"
      && options.utils.osType !== undefined
      ? "utils-ok" : "utils-missing";
  }
  if (text === "global") {
    return typeof oxidict === "object" ? "has-oxidict" : "no-oxidict";
  }
  return text + "|" + from + "|" + to;
}
"#;

/// 进程级 config 单例只能设置一次，全部断言收在一个测试里。
#[test]
fn plugin_runtime_end_to_end() {
    let root = std::env::temp_dir().join(format!("oxidict-plugin-test-{}", std::process::id()));
    let plugin_dir = root.join("plugins").join("translate").join("plugin.test");
    std::fs::create_dir_all(&plugin_dir).expect("创建插件目录失败");
    std::fs::write(plugin_dir.join("main.js"), MAIN_JS).expect("写入 main.js 失败");

    let store = ConfigStore::init_with_path(root.join("config.json")).expect("初始化配置失败");
    oxidict_core::config::set_global(store);

    // 加载
    runtime::load("plugin.test", &plugin_dir).expect("插件加载失败");

    // 测试线程不在 tokio 上下文里，用全局 runtime 句柄阻塞等待。
    let call = |input: &str, config: serde_json::Map<String, serde_json::Value>| {
        oxidict_core::runtime::handle().block_on(runtime::call(CallRequest {
            plugin_id: "plugin.test".into(),
            func: "translate",
            input: input.into(),
            from: "en".into(),
            to: "zh".into(),
            config,
        }))
    };

    // 1. 同步返回
    let out = call("hello", Default::default()).expect("调用失败");
    assert_eq!(out, serde_json::json!("hello|en|zh"));

    // 2. async 返回（JS Promise → Rust future）
    let out = call("async", Default::default()).expect("调用失败");
    assert_eq!(out, serde_json::json!("ASYNC:en:zh"));

    // 3. 对象回传
    let out = call("dict", Default::default()).expect("调用失败");
    assert_eq!(
        out.get("explanations")
            .and_then(|v| v.as_array())
            .map(|a| a.len()),
        Some(2)
    );

    // 4. config / detect 注入
    let mut cfg = serde_json::Map::new();
    cfg.insert("apiKey".into(), serde_json::json!("sk-test"));
    let out = call("config", cfg).expect("调用失败");
    assert_eq!(out, serde_json::json!("apiKey=sk-test,detect=en"));

    // 5. utils 注入
    let out = call("utils", Default::default()).expect("调用失败");
    assert_eq!(out, serde_json::json!("utils-ok"));

    // 6. 全局 oxidict 对象
    let out = call("global", Default::default()).expect("调用失败");
    assert_eq!(out, serde_json::json!("has-oxidict"));

    // 7. JS 异常透传（应包含 throw 的 message）
    let err = call("err", Default::default()).expect_err("应返回错误");
    assert!(err.to_string().contains("boom"), "实际错误: {err}");

    // 8. 未加载的插件报清晰错误
    let err = oxidict_core::runtime::handle()
        .block_on(runtime::call(CallRequest {
            plugin_id: "plugin.missing".into(),
            func: "translate",
            input: "x".into(),
            from: "en".into(),
            to: "zh".into(),
            config: Default::default(),
        }))
        .expect_err("应返回错误");
    assert!(
        err.to_string().contains("plugin.missing"),
        "实际错误: {err}"
    );

    // 9. 卸载后再调用报错
    runtime::unload("plugin.test");
    let err = call("hello", Default::default()).expect_err("应返回错误");
    assert!(err.to_string().contains("未加载"), "实际错误: {err}");

    let _ = std::fs::remove_dir_all(&root);
}
