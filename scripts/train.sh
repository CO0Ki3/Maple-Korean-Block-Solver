#!/bin/sh
# CEM 학습. 사용: scripts/train.sh <출력이름> [시작 가중치.json] [세대수] [손패상한]
# 결과: weights/runs/<출력이름>/{train.log, gen_XXX.json, latest.json, best.json}
set -e
cd "$(dirname "$0")/.."
NAME="${1:?출력 이름이 필요합니다}"; START="${2:-}"; GENS="${3:-40}"; HANDS="${4:-200}"
[ -x target/release/moamoa ] || cargo build --release -p moamoa-cli
mkdir -p "weights/runs/$NAME"
THREADS=$(( $(sysctl -n hw.ncpu 2>/dev/null || echo 8) - 1 ))
if [ -n "$START" ]; then
  nohup ./target/release/moamoa train --weights "$START" --sigma 0.15 --noise 0.5 --pop 32 --elite 6 --gens "$GENS" --games 10 --val-games 24 --beam 12 --reroll-beam 6 --max-hands "$HANDS" --threads "$THREADS" --out "weights/runs/$NAME" > "weights/runs/$NAME/train.log" 2>&1 &
else
  nohup ./target/release/moamoa train --pop 32 --elite 6 --gens "$GENS" --games 10 --val-games 24 --beam 12 --reroll-beam 6 --max-hands "$HANDS" --threads "$THREADS" --out "weights/runs/$NAME" > "weights/runs/$NAME/train.log" 2>&1 &
fi
echo "학습 시작 (pid $!). 진행: tail -f weights/runs/$NAME/train.log"
