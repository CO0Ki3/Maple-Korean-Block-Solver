#!/bin/sh
# 학습 결과를 저장소와 UI 기본 가중치로 반영한다. 사용: scripts/publish-weights.sh weights/runs/<이름>/latest.json
set -e
cd "$(dirname "$0")/.."
SRC="${1:?가중치 파일이 필요합니다}"
cp "$SRC" weights/trained.json
mkdir -p web/public/weights && cp "$SRC" web/public/weights/trained.json
echo "반영됨: $SRC → weights/trained.json, web/public/weights/trained.json (웹은 다시 빌드/새로고침)"
