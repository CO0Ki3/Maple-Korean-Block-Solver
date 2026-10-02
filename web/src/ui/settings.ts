// 설정: 탐색 강도, 회전·반전 규약, 평가 가중치, 조각 가중치, 워커 수.
import { app } from '../app';
import { defaultWeights, featureNames } from '../engine/wasm';
import { store } from '../store';
import { NUM_PIECES } from '../types';
import { esc } from './mini';

const LABELS: Record<string, string> = {
  filled: '채워진 칸 수',
  nonempty_rows: '비어 있지 않은 줄 수',
  row_trans: '가로 들쭉날쭉(전이)',
  col_trans: '세로 들쭉날쭉(전이)',
  isolated: '사방이 막힌 빈 칸',
  narrow: '폭 1 통로 빈 칸',
  rows_missing_1: '빈 칸 1개인 줄',
  rows_missing_2: '빈 칸 2개인 줄',
  rows_missing_3: '빈 칸 3개인 줄',
  well_max: '1열 우물 최대 길이',
  well_sq: '1열 우물 길이² 합',
  pair_near_full: '이웃한 거의 찬 줄 쌍',
  empty_components: '빈 영역 조각 수',
  small_components: '크기 ≤ 2 빈 영역',
  icon_count: '판 위 아이콘 수',
  icon_row_missing: '아이콘 줄의 빈 칸 수',
  dots: '보유 점 찍기',
  rerolls: '보유 바꿔 뽑기',
  held_sqrt: '√보유 능력',
  at_cap: '보유 7개(생성 차단)',
  max_row_fill: '가장 찬 줄의 칸 수',
  edge_empty: '맨 위·아래 줄 빈 칸',
  fit_risk: '놓을 자리 없는 조각 확률 합',
  dead_coverage: '덮기 어려운 빈 칸(확률 가중)',
};

export function renderSettings(): void {
  const el = document.getElementById('settings')!;
  const s = store.state.settings;
  const p = s.params;
  const names = featureNames();
  const dw = defaultWeights();
  const w = s.weights ?? dw;
  let h = `<div class="grid2">
    <label>탐색 빔 <input type="number" id="set-beam" value="${p.beam ?? 64}" min="4" max="512" /></label>
    <label>점 찍기 끼워 넣기 최대 <input type="number" id="set-maxdots" value="${p.max_dots ?? 2}" min="0" max="3" /></label>
    <label>리프 정밀 평가 수 <input type="number" id="set-leafk" value="${p.leaf_k ?? 48}" min="4" max="400" /></label>
    <label>다음 손패 샘플 수 (0=끔, 느려짐) <input type="number" id="set-samples" value="${p.samples ?? 0}" min="0" max="32" /></label>
    <label>상위 후보 추가 샘플 (2단계) <input type="number" id="set-stop" value="${p.samples_top ?? 0}" min="0" max="64" /></label>
    <label>샘플 탐색 빔 <input type="number" id="set-sbeam" value="${p.sample_beam ?? 12}" min="2" max="64" /></label>
    <label>워커 수 <input type="number" id="set-workers" value="${s.workers}" min="1" max="16" /></label>
    <label>회전 버튼 방향 <select id="set-rot"><option value="cw"${s.rotDir === 'cw' ? ' selected' : ''}>시계 방향</option><option value="ccw"${s.rotDir === 'ccw' ? ' selected' : ''}>반시계 방향</option></select></label>
    <label>반전 버튼 축 <select id="set-flip"><option value="h"${s.flipAxis === 'h' ? ' selected' : ''}>좌우 뒤집기</option><option value="v"${s.flipAxis === 'v' ? ' selected' : ''}>상하 뒤집기</option></select></label>
    <label>라플라스 보정 α <input type="number" id="set-alpha" value="${s.alpha}" min="0" max="50" step="0.5" /></label>
    <label><input type="checkbox" id="set-auto"${s.autoSolve ? ' checked' : ''} /> 손패가 차면 자동 계산</label>
  </div>
  <h3>평가 가중치 <span class="small muted">(${esc(s.weightsLabel)})</span></h3>
  <div class="row gap"><button id="w-default">기본값</button><button id="w-trained">학습된 가중치 불러오기</button><label class="link">JSON 파일<input id="w-file" type="file" accept="application/json" hidden /></label><button id="w-export">JSON 복사</button></div>
  <div class="weights">`;
  for (const n of names) {
    h += `<label title="${n}">${esc(LABELS[n] ?? n)} <input type="number" step="0.5" data-w="${n}" value="${w[n] ?? 0}" /></label>`;
  }
  h += `</div>
  <h3>조각 등장 횟수</h3>
  <div class="row gap"><button id="c-reset">횟수 0으로</button><button id="c-export">횟수 JSON 복사</button><label class="link">횟수 JSON 불러오기<input id="c-file" type="file" accept="application/json" hidden /></label></div>
  <p class="small muted">바꿔 뽑기·다음 조각 예측에 쓰는 확률은 (횟수 + α) ∝ 로 계산합니다. 단계별 확률은 쓰지 않습니다.</p>
  <h3>저장</h3>
  <div class="row gap"><button id="s-export">전체 상태 JSON 복사</button><label class="link">상태 JSON 불러오기<input id="s-file" type="file" accept="application/json" hidden /></label><button id="s-reset" class="danger">모두 초기화</button></div>`;
  el.innerHTML = h;

  const num = (id: string) => el.querySelector<HTMLInputElement>(`#${id}`)!;
  const onParams = () => {
    s.params.beam = clamp(+num('set-beam').value, 4, 512, 64);
    s.params.max_dots = clamp(+num('set-maxdots').value, 0, 3, 2);
    s.params.leaf_k = clamp(+num('set-leafk').value, 4, 400, 48);
    s.params.samples = clamp(+num('set-samples').value, 0, 32, 0);
    s.params.samples_top = clamp(+num('set-stop').value, 0, 64, 0);
    s.params.sample_beam = clamp(+num('set-sbeam').value, 2, 64, 12);
    s.alpha = clamp(+num('set-alpha').value, 0, 50, 1);
    s.autoSolve = num('set-auto').checked;
    store.save();
    app.invalidate();
    app.renderAll();
    app.scheduleSolve();
  };
  for (const id of ['set-beam', 'set-maxdots', 'set-leafk', 'set-samples', 'set-stop', 'set-sbeam', 'set-alpha', 'set-auto']) num(id).onchange = onParams;
  num('set-workers').onchange = () => {
    s.workers = clamp(+num('set-workers').value, 1, 16, 4);
    store.save();
    location.reload();
  };
  el.querySelector<HTMLSelectElement>('#set-rot')!.onchange = (e) => {
    s.rotDir = (e.target as HTMLSelectElement).value as 'cw' | 'ccw';
    store.save();
    app.renderAll();
  };
  el.querySelector<HTMLSelectElement>('#set-flip')!.onchange = (e) => {
    s.flipAxis = (e.target as HTMLSelectElement).value as 'h' | 'v';
    store.save();
    app.renderAll();
  };
  for (const inp of el.querySelectorAll<HTMLInputElement>('[data-w]')) {
    inp.onchange = () => {
      const cur = { ...(s.weights ?? dw) };
      cur[inp.dataset.w!] = +inp.value || 0;
      s.weights = cur;
      s.weightsLabel = '직접 수정';
      store.save();
      app.invalidate();
      app.renderAll();
      app.scheduleSolve();
    };
  }
  el.querySelector<HTMLButtonElement>('#w-default')!.onclick = () => setWeights(null, '기본(선택)');
  el.querySelector<HTMLButtonElement>('#w-trained')!.onclick = async () => {
    try {
      const res = await fetch('./weights/trained.json', { cache: 'no-store' });
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const j = (await res.json()) as Record<string, number>;
      setWeights(j, '학습됨 (weights/trained.json)');
    } catch (e) {
      app.status(`학습 가중치를 찾지 못했습니다: ${String(e)}`, 'err');
    }
  };
  el.querySelector<HTMLInputElement>('#w-file')!.onchange = async (e) => {
    const f = (e.target as HTMLInputElement).files?.[0];
    if (!f) return;
    try {
      const j = JSON.parse(await f.text()) as Record<string, number>;
      setWeights(j.w ? (j as unknown as { w: Record<string, number> }).w : j, f.name);
    } catch (err) {
      app.status(`가중치 JSON 오류: ${String(err)}`, 'err');
    }
  };
  el.querySelector<HTMLButtonElement>('#w-export')!.onclick = () => copy(JSON.stringify(s.weights ?? dw, null, 2), '가중치를 복사했습니다.');
  el.querySelector<HTMLButtonElement>('#c-reset')!.onclick = () => {
    store.snapshot();
    s.counts = Array.from({ length: NUM_PIECES }, () => 0);
    store.save();
    app.renderAll();
  };
  el.querySelector<HTMLButtonElement>('#c-export')!.onclick = () => {
    const obj: Record<string, number> = {};
    app.pieces.forEach((p) => (obj[p.name] = s.counts[p.id]));
    copy(JSON.stringify(obj), '횟수를 복사했습니다.');
  };
  el.querySelector<HTMLInputElement>('#c-file')!.onchange = async (e) => {
    const f = (e.target as HTMLInputElement).files?.[0];
    if (!f) return;
    try {
      const j = JSON.parse(await f.text()) as Record<string, number> | number[];
      store.snapshot();
      if (Array.isArray(j) && j.length === NUM_PIECES) s.counts = j.map((x) => Math.max(0, Math.round(+x || 0)));
      else app.pieces.forEach((p) => (s.counts[p.id] = Math.max(0, Math.round(+(j as Record<string, number>)[p.name] || 0))));
      store.save();
      app.renderAll();
    } catch (err) {
      app.status(`횟수 JSON 오류: ${String(err)}`, 'err');
    }
  };
  el.querySelector<HTMLButtonElement>('#s-export')!.onclick = () => copy(JSON.stringify(store.state), '상태를 복사했습니다.');
  el.querySelector<HTMLInputElement>('#s-file')!.onchange = async (e) => {
    const f = (e.target as HTMLInputElement).files?.[0];
    if (!f) return;
    try {
      const text = await f.text();
      const j = JSON.parse(text) as { game?: { board?: unknown } };
      if (!j || !j.game || !Array.isArray(j.game.board) || j.game.board.length !== 16) throw new Error('game.board가 16줄이 아닙니다');
      localStorage.setItem('moamoa-v2', text);
      location.reload();
    } catch (err) {
      app.status(`상태 JSON 오류: ${String(err)}`, 'err');
    }
  };
  el.querySelector<HTMLButtonElement>('#s-reset')!.onclick = () => {
    if (!confirm('판, 손패, 횟수, 설정을 모두 지웁니다. 계속할까요?')) return;
    localStorage.removeItem('moamoa-v2');
    location.reload();
  };
}

function setWeights(w: Record<string, number> | null, label: string): void {
  store.state.settings.weights = w;
  store.state.settings.weightsLabel = label;
  store.save();
  app.invalidate();
  app.renderAll();
  app.status(`평가 가중치: ${label}`, 'ok');
  app.scheduleSolve();
}

function clamp(v: number, lo: number, hi: number, def: number): number {
  if (!Number.isFinite(v)) return def;
  return Math.max(lo, Math.min(hi, v));
}

function copy(text: string, okMsg: string): void {
  navigator.clipboard.writeText(text).then(
    () => app.status(okMsg, 'ok'),
    () => {
      prompt('복사할 내용입니다.', text);
    },
  );
}
