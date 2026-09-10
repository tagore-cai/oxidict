#!/bin/bash
set -euo pipefail
APP_NAME="Oxidict"
BUNDLE="target/release/bundle/macos"
APP_DIR="$BUNDLE/$APP_NAME.app"
CONTENTS="$APP_DIR/Contents"
echo "=== 构建 .app Bundle ==="
echo "[1/4] cargo build --release"
cargo build --release -p oxidict 2>&1 | tail -1
echo "[2/4] 创建 Bundle 目录"
rm -rf "$APP_DIR"
mkdir -p "$CONTENTS/MacOS" "$CONTENTS/Resources"
echo "[3/4] 拷贝文件"
cp target/release/oxidict "$CONTENTS/MacOS/oxidict"
cp Info.plist "$CONTENTS/Info.plist"
if [ -f "assets/icon.icns" ]; then
  cp assets/icon.icns "$CONTENTS/Resources/icon.icns"
else
  echo "warn: assets/icon.icns 不存在，使用系统默认图标"
  echo "      生成方式：把 1024x1024 png 放入临时目录，执行"
  echo "      iconutil -c icns assets/icon.iconset -o assets/icon.icns"
  /usr/libexec/PlistBuddy -c "Delete :CFBundleIconFile" "$CONTENTS/Info.plist" 2>/dev/null || true
fi
echo -n "APPL????" > "$CONTENTS/PkgInfo"
echo "[4/4] 完成"
echo ""
echo "Bundle: $APP_DIR"
du -sh "$APP_DIR"
echo "启动: open $APP_DIR"
echo "制作 dmg: hdiutil create -srcfolder $APP_DIR -volname Oxidict target/release/bundle/Oxidict.dmg"
