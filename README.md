# 한글 모아모아 솔버 v2

메이플스토리 「한글 모아모아」 이벤트(2026-10-01 ~ 10-14, 10×16 블록 퍼즐) 솔버입니다.
이전 버전(단일 HTML, https://github.com/CO0Ki3/Maple-Korean-Block-Solver)을 Rust 엔진 + WASM + TypeScript UI로 다시 만들었고,
이벤트 규칙(실제 점수표 300·n², 능력 생성·한도·소멸, 점 찍기·바꿔 뽑기, 종료 조건, 50만 점 캡)을 전부 시뮬레이터에 넣었습니다.
규칙 명세와 설계는 `docs/DESIGN.md`에 있습니다.

## 구성

```
crates/engine   규칙·상태 전이·평가 함수·빔 탐색·자기대전 (순수 Rust, 테스트 포함)
crates/cli      moamoa 바이너리: selfplay / bench / train(CEM) / pieces
crates/wasm     wasm-bindgen 바인딩 (JSON 입출력)
web/            Vite + TypeScript UI, 워커 풀, 스크린샷 인식
weights/        학습된 평가 가중치
docs/           설계 문서
```

## 실행 (개발 환경 없이)

[Releases](https://github.com/CO0Ki3/Maple-Korean-Block-Solver/releases)에서 `moamoa-solver-vX.Y.Z.zip`을 받아 풀고, [Node.js LTS](https://nodejs.org)만 설치한 뒤
Windows는 `start.bat`, macOS는 `start.command`를 더블클릭하면 브라우저가 http://localhost:8787 로 열립니다. 자세한 내용은 zip 안의 `README.txt`.
(배포 zip 만들기: `scripts/build-release.sh v2.0.0` → `release/moamoa-solver-v2.0.0.zip`)

## 실행 (소스에서)

필요: Rust(stable), `wasm-pack`, Node 20+.

```sh
# 1) 엔진 테스트
cargo test --release -p moamoa-engine

# 2) wasm 빌드 → web/src/pkg
cd web && npm install && npm run wasm

# 3) 개발 서버 (브라우저에서 http://localhost:5173)
npm run dev
# 또는 정적 빌드
npm run build && npm run preview
```

## UI 사용법

1. 게임 화면을 캡처해 페이지에 붙여넣으면(⌘V / Ctrl+V) 판·손패·능력 아이콘을 읽고 바로 계산합니다. 격자를 못 찾으면 「판 위치 직접 지정」으로 판의 왼쪽 위·오른쪽 아래를 클릭하세요.
2. 직접 입력도 됩니다. 판은 「칸 칠하기」 모드에서 드래그, 아이콘은 「능력 아이콘」 모드에서 클릭(없음 → ⊙ → ⇄), 손패는 슬롯을 고르고 라이브러리의 조각을 클릭.
3. 추천 배치는 단계별로 조각 이름, 회전·반전 버튼 횟수, 왼쪽 위 기준 위치, 제거 줄 수와 점수를 보여줍니다. 단계를 클릭하면 판에 미리보기가 뜹니다.
4. 게임에 그대로 놓았으면 「1단계 적용」/「전부 적용」으로 판을 이어가거나, 다음 스크린샷을 붙여넣으면 추천대로 놓았는지 자동으로 맞춰 줍니다.
5. 「바꿔 뽑기 분석」은 슬롯마다 19종 조각으로 바꿨을 때의 기대 평가를 보여줍니다. 보유 능력이 7개면 새 아이콘이 생기지 않으므로 쓰는 편이 이득이라고 알려줍니다.
6. 점 찍기는 추천 계획에 자동으로 끼워 넣습니다(설정에서 최대 횟수 조절). 직접 쓰려면 「점 찍기 사용」 모드에서 칸을 클릭.
7. 설정에서 탐색 빔, 다음 손패 샘플 수(켜면 느려지지만 다음 조각이 안 들어갈 위험을 직접 측정), 회전 방향·반전 축, 평가 가중치, 조각 등장 횟수를 바꿀 수 있습니다. 판·손패·설정은 브라우저(localStorage)에 저장됩니다.

## 시뮬레이터 탭

상단의 「시뮬레이터」 탭에서 규칙 그대로 게임을 자동으로 둡니다. 손패 3개가 조각 가중치대로 들어오고, 솔버와 같은 정책(빔 탐색 + 계획에 끼워 넣는 점 찍기 + 손패를 다 못 놓거나 보유가 7개로 찼을 때의 바꿔 뽑기 기대값 판단)으로 배치합니다. 7번째 배치마다 아이콘이 생기고, 3개를 넘으면 가장 오래된 아이콘이 사라지는 것까지 그대로입니다.

- 「새 게임」 → 「한 수」로 한 수씩 보거나 「자동 진행」으로 끝까지. 속도를 「최고」로 두면 애니메이션 없이 수당 20~40ms로 돌아갑니다.
- 오른쪽 진행 기록에 배치·줄 제거·능력 획득/생성/소멸·바꿔 뽑기·점 찍기·종료 사유가 남습니다.
- 「여러 판 일괄 실행」은 워커 여러 개에 판을 나눠 끝까지 두고 평균·중앙값·최고·히스토그램·판별 표를 보여줍니다. 표의 「다시 보기」로 그 시드를 시뮬레이터에서 재생할 수 있습니다.
- 정책 코드는 네이티브 자기대전(`moamoa selfplay`)과 같은 Rust 함수(`policy_step`)라 두 결과가 같은 시드에서 일치합니다.

## 엔진과 학습

- 판은 행당 u16 비트마스크 16개. 조각 19종의 회전·반전 방향은 미리 계산합니다.
- 의사결정: 손패 3개의 배치 순서·위치를 빔 서치로 전개하고(점 찍기 행동 포함), 리프는 실제 점수 + 학습된 선형 평가 함수로 평가합니다. 옵션으로 다음 손패를 샘플링해 얕은 탐색을 덧붙입니다.
- 평가 특징 24개(채움, 전이, 고립 칸, 줄 완성도, 1열 우물 길이, 빈 영역 분할, 아이콘 줄, 보유 능력, 조각별 「놓을 자리 없음」 확률, 커버리지 기반 사각 등). 가중치는 교차 엔트로피 방법(CEM)으로 자기대전 평균 점수를 올리는 방향으로 학습합니다.

```sh
# 자기대전 기준 측정 (12스레드, 24판)
./target/release/moamoa selfplay --games 24 --beam 32 --threads 12 --verbose
# 학습 (세대마다 weights/runs/<이름>/gen_XXX.json, latest.json 저장; best.json은 고정 검증 시드 --val-games 판 기준)
./target/release/moamoa train --pop 32 --elite 6 --gens 40 --games 8 --beam 12 --max-hands 120 --val-games 24 --threads 11 --out weights/runs/cem1
# 이어서 학습하려면 --weights 로 시작점을 준다 (예: --weights weights/trained.json --sigma 0.15 --noise 0.5)
# 학습된 가중치로 측정
./target/release/moamoa selfplay --games 24 --beam 32 --weights weights/runs/cem1/best.json
```

웹 UI는 시작할 때 `web/public/weights/trained.json`을 자동으로 읽습니다(설정에서 기본값으로 되돌릴 수 있음). 학습 결과를 반영하려면 `scripts/publish-weights.sh <파일>`을 쓰면 됩니다.

편의 스크립트: `scripts/eval.sh [가중치|default] [판수] [빔] [시드]`, `scripts/train.sh <이름> [시작가중치] [세대] [손패상한]`, `scripts/publish-weights.sh <파일>`.

## 성능 (자기대전, 고정 시드)

| 구성 | 평균 | 중앙값 | 비고 |
|---|---|---|---|
| 기본 가중치, 빔 32 | 203,383 | 188,122 | 48판 |
| 학습 가중치(trained.json), 빔 32 | 322,598 | 330,106 | 48판, 캡 도달 20판 |
| 학습 가중치 + 다음 손패 샘플 4 | 444,506 | 500,000 | 24판(1차 가중치 기준), 캡 도달 19판 |

이전 솔버의 최고 기록은 14만 점이었습니다. 자세한 표는 `weights/README.md`.

## 가정 (확인되면 고칠 것)

- 아이콘 생성 위치는 아이콘 없는 빈 칸 중 균등 랜덤.
- 보유 7개라 생성이 건너뛰어져도 배치 카운터는 계속 돌아간다.
- 바꿔 뽑기 결과는 같은 조각일 수도 있다.
- 조각 등장 확률은 단계(누적 줄 수)와 무관하게 「관측 횟수 + α」로 둔다.
