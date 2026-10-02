#!/bin/sh
# 가중치 파일을 고정 시드로 평가한다. 사용: scripts/eval.sh [weights.json|default] [games] [beam] [seed]
set -e
cd "$(dirname "$0")/.."
W="${1:-default}"; GAMES="${2:-48}"; BEAM="${3:-32}"; SEED="${4:-100}"
[ -x target/release/moamoa ] || cargo build --release -p moamoa-cli
if [ "$W" = "default" ]; then
  ./target/release/moamoa selfplay --games "$GAMES" --seed "$SEED" --beam "$BEAM" --threads "$(sysctl -n hw.ncpu 2>/dev/null || echo 8)"
else
  ./target/release/moamoa selfplay --games "$GAMES" --seed "$SEED" --beam "$BEAM" --threads "$(sysctl -n hw.ncpu 2>/dev/null || echo 8)" --weights "$W"
fi
