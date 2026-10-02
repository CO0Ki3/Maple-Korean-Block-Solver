#!/bin/sh
# macOS: 더블클릭하면 터미널이 열리고 서버가 뜹니다. (처음에 "확인되지 않은 개발자" 경고가 나오면 우클릭 → 열기)
cd "$(dirname "$0")" || exit 1
if ! command -v node >/dev/null 2>&1; then
  echo "Node.js 가 필요합니다. https://nodejs.org 에서 LTS 버전을 설치한 뒤 다시 실행하세요."
  echo "아무 키나 누르면 닫힙니다."; read -r _; exit 1
fi
node serve.js
echo "서버가 종료되었습니다. 아무 키나 누르면 닫힙니다."; read -r _
