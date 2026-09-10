#!/usr/bin/env python3
"""统计 oxidict-rs 项目关键指标，用于上下文压缩时的状态快照。
"""
import pathlib, re

root = pathlib.Path(__file__).resolve().parent.parent

# 1. 服务注册数
svc = root / "crates" / "oxidict-services" / "src"
counts = {}
for mod in sorted(svc.glob("*/mod.rs")):
    kind = mod.parent.name
    text = mod.read_text()
    n = len(re.findall(r"register_translator|register_recognizer|register_tts|register_collector", text))
    counts[kind] = n

total = sum(counts.values())
print("服务注册数:", counts, "总计:", total)

# 2. 语言
loc = root / "crates" / "oxidict-core" / "locales"
langs = sorted(d.name for d in loc.iterdir() if d.is_dir())
print(f"语言: {len(langs)} 种: {langs}")

# 3. config keys
cfg = (root / "crates" / "oxidict-core" / "src" / "config.rs").read_text()
pat = r'pub const (\w+): &str = "([^"]+)"'
keys = sorted(set(re.findall(pat, cfg)))
print(f"config keys: {len(keys)}")

# 4. UI 模块
ui = root / "crates" / "oxidict-ui" / "src"
print("UI 模块:", sorted(f.stem for f in ui.glob("*.rs")))

# 5. 平台模块
plat = root / "crates" / "oxidict-platform" / "src"
print("platform 模块:", sorted(f.stem for f in plat.glob("*.rs")))

# 6. 代码量
files = list(root.glob("crates/**/*.rs")) + list(root.glob("app/**/*.rs"))
lines = sum(len(f.read_text(errors="ignore").splitlines()) for f in files)
print(f"Rust 文件 {len(files)} 个, 共 {lines} 行.")

# 7. 服务实现文件数（每类实际 .rs，排除 mod.rs
for kind in ["translate", "recognize", "tts", "collection"]:
    d = svc / kind
    n = len([f for f in d.glob("*.rs") if f.stem != "mod"])
    print(f"  {kind}: {n} 个实现文件")

print("---")
print("locale 语言列表完整:", len(langs) == 20)

# 8. git
import subprocess
log = subprocess.run(["git", "log", "--oneline"], cwd=root, capture_output=True, text=True).stdout
print("git commits:")
print(log)

st = subprocess.run(["git", "status", "--porcelain"], cwd=root, capture_output=True, text=True).stdout
print("工作区改动:", st.strip() or "(clean)")

# 9. 服务实现清单（用于核对对齐）
print("--- 翻译服务清单 ---")
for f in sorted((svc / "translate").glob("*.rs")):
    if f.stem == "mod":
        continue
    print(f"  {f.stem}")
