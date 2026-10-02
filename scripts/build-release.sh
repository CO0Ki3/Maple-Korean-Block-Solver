#!/bin/sh
# 배포용 zip 생성: Node만 있으면 실행되는 묶음 (빌드 결과물 + 로컬 서버 + 실행 스크립트)
# 사용: scripts/build-release.sh [버전]   → release/moamoa-solver-<버전>.zip
set -e
cd "$(dirname "$0")/.."
VER="${1:-v2.0.0}"
( cd web && npm install --no-audit --no-fund >/dev/null && npm run wasm >/dev/null 2>&1 && npm run build >/dev/null )
OUT="release/moamoa-solver-$VER"
rm -rf "$OUT" && mkdir -p "$OUT"
cp -R web/dist "$OUT/app"
cp release-template/serve.js release-template/start.command release-template/start.bat release-template/README.txt "$OUT/"
chmod +x "$OUT/start.command"
( cd release && rm -f "moamoa-solver-$VER.zip" && zip -qr "moamoa-solver-$VER.zip" "moamoa-solver-$VER" )
echo "만들어짐: release/moamoa-solver-$VER.zip ($(du -h "release/moamoa-solver-$VER.zip" | cut -f1))"
