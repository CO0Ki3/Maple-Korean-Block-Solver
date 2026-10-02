// 시뮬레이터 화면.
import { app } from '../app';
import { simulator, summarize, type Speed } from '../sim/simulator';
import type { GameResult } from '../types';
import { ABILITY_CAP } from '../types';
import { gridHtml } from './grid';
import { esc, fmt, miniShape } from './mini';

let wired = false;

export function renderSim(): void {
  const view = document.getElementById('simView');
  if (!view || view.hidden) return;
  renderControls();
  renderBoardAndHand();
  renderStats();
  renderLog();
  renderBatch();
}

function renderControls(): void {
  const el = document.getElementById('sim-controls')!;
  const s = simulator;
  if (!wired) {
    el.innerHTML = `
      <div class="row gap">
        <label>시드 <input type="number" id="sim-seed" value="${s.seed}" min="0" step="1" style="width:90px" /></label>
        <label>속도 <select id="sim-speed">
          <option value="slow">느리게</option><option value="normal" selected>보통</option><option value="fast">빠르게</option><option value="max">최고(애니메이션 없음)</option>
        </select></label>
        <label>탐색 빔 <input type="number" id="sim-beam" value="${s.settings.beam}" min="4" max="256" style="width:70px" /></label>
        <label>점 찍기 끼워 넣기 <input type="number" id="sim-maxdots" value="${s.settings.maxDots}" min="0" max="3" style="width:60px" /></label>
        <label title="켜면 수당 계산이 5~8배 느려지지만 점수가 크게 오릅니다 (자기대전 비교: 평균 32.8만 → 44.5만)">다음 손패 샘플 <input type="number" id="sim-samples" value="${s.settings.samples}" min="0" max="16" style="width:60px" /></label>
        <label title="끄면 19종 균등 분포. 켜면 설정의 조각 등장 횟수(+α)를 분포로 씁니다. 관측이 적으면 분포가 크게 쏠리니 주의"><input type="checkbox" id="sim-counts"${s.settings.usePieceCounts ? ' checked' : ''} /> 기록된 등장 횟수로 추첨</label>
      </div>
      <div id="sim-desc" class="small muted" style="margin-top:6px"></div>
      <div class="row gap">
        <button id="sim-new">새 게임</button>
        <button id="sim-step">한 수</button>
        <button id="sim-run" class="primary">자동 진행</button>
        <button id="sim-pause">일시정지</button>
        <button id="sim-next-seed" title="시드를 1 올려 새 게임">다음 시드</button>
      </div>
      <p class="small muted" style="margin:6px 0 0">솔버와 같은 정책으로 둡니다: 손패 3개를 빔 탐색으로 배치하고, 점 찍기는 계획에 끼워 넣으며, 손패를 다 못 놓거나 보유 능력이 7개로 가득 차면 바꿔 뽑기 기대값을 따져 사용합니다. 평가 가중치와 조각 가중치는 설정 패널 값을 따릅니다.</p>`;
    const seedEl = el.querySelector<HTMLInputElement>('#sim-seed')!;
    el.querySelector<HTMLSelectElement>('#sim-speed')!.onchange = (e) => {
      s.speed = (e.target as HTMLSelectElement).value as Speed;
    };
    el.querySelector<HTMLInputElement>('#sim-beam')!.onchange = (e) => {
      s.settings.beam = Math.max(4, Math.min(256, +(e.target as HTMLInputElement).value || 32));
    };
    el.querySelector<HTMLInputElement>('#sim-maxdots')!.onchange = (e) => {
      s.settings.maxDots = Math.max(0, Math.min(3, +(e.target as HTMLInputElement).value || 0));
    };
    el.querySelector<HTMLInputElement>('#sim-samples')!.onchange = (e) => {
      s.settings.samples = Math.max(0, Math.min(16, +(e.target as HTMLInputElement).value || 0));
      renderSim();
    };
    el.querySelector<HTMLInputElement>('#sim-counts')!.onchange = (e) => {
      s.settings.usePieceCounts = (e.target as HTMLInputElement).checked;
      renderSim();
    };
    el.querySelector<HTMLButtonElement>('#sim-new')!.onclick = () => {
      s.stopAll();
      s.newGame(Math.max(0, Math.round(+seedEl.value || 0)));
    };
    el.querySelector<HTMLButtonElement>('#sim-next-seed')!.onclick = () => {
      s.stopAll();
      seedEl.value = String(s.seed + 1);
      s.newGame(s.seed + 1);
    };
    el.querySelector<HTMLButtonElement>('#sim-step')!.onclick = () => {
      if (!s.state) s.newGame(Math.max(0, Math.round(+seedEl.value || 0)));
      void s.step();
    };
    el.querySelector<HTMLButtonElement>('#sim-run')!.onclick = () => {
      if (!s.state) s.newGame(Math.max(0, Math.round(+seedEl.value || 0)));
      void s.run();
    };
    el.querySelector<HTMLButtonElement>('#sim-pause')!.onclick = () => s.pause();
    wired = true;
  }
  const desc = el.querySelector<HTMLElement>('#sim-desc');
  if (desc) desc.textContent = `지금 설정: ${s.describe()} (네이티브 CLI 평가와 비교하려면 조각 분포를 균등으로)`;
  const running = s.running || s.busy;
  (el.querySelector('#sim-run') as HTMLButtonElement).disabled = s.running;
  (el.querySelector('#sim-pause') as HTMLButtonElement).disabled = !s.running;
  (el.querySelector('#sim-step') as HTMLButtonElement).disabled = running || !!s.state?.over;
}

function renderBoardAndHand(): void {
  const s = simulator;
  const boardEl = document.getElementById('sim-board')!;
  const handEl = document.getElementById('sim-hand')!;
  const st = s.state;
  if (!st) {
    boardEl.innerHTML = gridHtml(Array.from({ length: 16 }, () => '.'.repeat(10)), []);
    handEl.innerHTML = '<div class="small muted">「새 게임」을 누르면 손패 3개가 들어옵니다.</div>';
    return;
  }
  const f = s.frame;
  const board = f ? f.board : st.board;
  const icons = f ? f.icons : st.icons;
  const hand = f ? f.hand : st.hand;
  boardEl.innerHTML = gridHtml(board, icons, { hl: f?.hl, rows: f?.rows });
  let h = '';
  hand.forEach((pid, i) => {
    const p = pid === null ? null : app.piece(pid);
    h += `<div class="slot${p ? '' : ' empty'}"><span class="lbl">슬롯 ${i + 1}</span>${p ? `${miniShape(p.cells, 11)}<span class="small"><b>${esc(p.name)}</b> ${p.n}칸</span>` : '<span class="small muted" style="margin:auto 0">배치됨</span>'}</div>`;
  });
  handEl.innerHTML = h;
}

function renderStats(): void {
  const s = simulator;
  const el = document.getElementById('sim-stats')!;
  const st = s.state;
  if (!st) {
    el.innerHTML = '<div class="banner soft">「새 게임」 → 「자동 진행」으로 한 판을 보면서 둘 수 있습니다. 「여러 판 일괄 실행」은 워커 안에서 화면 없이 돌고, 그동안 여기서는 같은 설정으로 한 판씩 자동 관전합니다.</div>';
    return;
  }
  const held = st.dots + st.rerolls;
  const r = s.stats;
  const perStep = s.stepCount ? s.elapsedMs / s.stepCount : 0;
  el.innerHTML = `
    <div class="statgrid">
      <div><span>점수</span><b class="num">${fmt(st.score)}</b></div>
      <div><span>제거한 줄</span><b class="num">${fmt(st.lines)}</b></div>
      <div><span>손패 / 배치</span><b class="num">${r.hands} / ${st.placements}</b></div>
      <div><span>동시 제거 1·2·3·4·5줄</span><b class="num">${r.clears.slice(1).join(' · ')}</b></div>
      <div><span>보유 ⊙ / ⇄</span><b class="num">${st.dots} / ${st.rerolls} <span class="muted">(${held}/${ABILITY_CAP})</span></b></div>
      <div><span>사용 ⊙ / ⇄ · 획득</span><b class="num">${r.dots_used} / ${r.rerolls_used} · ${r.pickups}</b></div>
      <div><span>판 위 아이콘</span><b class="num">${st.icons.length}/3</b></div>
      <div><span>다음 아이콘까지</span><b class="num">${st.placements % 7 === 0 ? 7 : 7 - (st.placements % 7)}번</b></div>
      <div><span>수 / 수당 계산</span><b class="num">${s.stepCount} / ${perStep.toFixed(0)}ms</b></div>
    </div>
    ${s.spectating ? `<div class="banner soft">일괄 실행 중 관전: 시드 ${s.seed}. 판이 끝나면 다음 시드로 이어집니다. 「일시정지」로 멈출 수 있습니다.</div>` : ''}
    ${st.over ? `<div class="banner warn">게임 종료 · 최종 ${fmt(st.score)}점 · ${fmt(st.lines)}줄 · 손패 ${r.hands}개${st.score >= 500000 ? ' · 50만 점 캡 도달' : ''}</div>` : ''}`;
}

function renderLog(): void {
  const el = document.getElementById('sim-log')!;
  const items = simulator.log.slice(-60).reverse();
  el.innerHTML = items.map((l) => `<div class="logline ${l.kind}"><span class="num muted">${l.step}</span> ${esc(l.text)}</div>`).join('');
}

function renderBatch(): void {
  const s = simulator;
  const el = document.getElementById('sim-batch')!;
  const out = document.getElementById('sim-batch-out')!;
  if (!el.dataset.wired) {
    el.innerHTML = `
      <div class="row gap">
        <label>판 수 <input type="number" id="batch-n" value="24" min="1" max="500" style="width:70px" /></label>
        <label>시작 시드 <input type="number" id="batch-seed" value="100" min="0" style="width:90px" /></label>
        <label>손패 상한 (0=끝까지) <input type="number" id="batch-hands" value="0" min="0" style="width:70px" /></label>
        <button id="batch-run" class="primary">일괄 실행</button>
        <button id="batch-stop">중단</button>
      </div>
      <p class="small muted" style="margin:6px 0 0">워커 여러 개에 판을 나눠 화면 없이 끝까지 둡니다(빔 32 기준 한 판에 수 초). 탐색 빔·샘플·분포는 위의 시뮬레이터 설정을 따르고, 그동안 왼쪽 화면에서는 같은 설정으로 한 판씩 자동 관전합니다. 특정 판은 표의 「다시 보기」로 재생합니다.</p>`;
    el.querySelector<HTMLButtonElement>('#batch-run')!.onclick = () => {
      const n = Math.max(1, Math.min(500, Math.round(+(el.querySelector('#batch-n') as HTMLInputElement).value || 1)));
      const seed = Math.max(0, Math.round(+(el.querySelector('#batch-seed') as HTMLInputElement).value || 0));
      const hands = Math.max(0, Math.round(+(el.querySelector('#batch-hands') as HTMLInputElement).value || 0));
      void s.runBatch(n, seed, hands);
    };
    el.querySelector<HTMLButtonElement>('#batch-stop')!.onclick = () => s.stopAll();
    el.dataset.wired = '1';
  }
  (el.querySelector('#batch-run') as HTMLButtonElement).disabled = s.batchRunning;
  const b = s.batch;
  if (!b) {
    out.innerHTML = '';
    return;
  }
  const sum = summarize(b.results);
  let h = `<div class="banner ${s.batchRunning ? 'soft' : 'ok'}">${s.batchRunning ? `진행 ${b.done}/${b.total}` : `완료 ${b.done}판`}${sum.games ? ` · 평균 <b class="num">${fmt(sum.mean)}</b> · 중앙값 ${fmt(sum.median)} · 최고 ${fmt(sum.max)} · 최저 ${fmt(sum.min)} · 평균 줄 ${sum.mean_lines.toFixed(0)} · 다중 제거 ${(sum.multi_ratio * 100).toFixed(1)}% · 캡 도달 ${sum.capped}` : ''}<div class="small muted">${esc(b.label)}</div></div>`;
  if (sum.games) {
    // 5만 점 구간 히스토그램
    const buckets = new Map<number, number>();
    for (const r of b.results) {
      const k = Math.min(10, Math.floor(r.score / 50000));
      buckets.set(k, (buckets.get(k) ?? 0) + 1);
    }
    const maxCnt = Math.max(...buckets.values());
    h += '<div class="hist">';
    for (let k = 0; k <= 10; k++) {
      const cnt = buckets.get(k) ?? 0;
      const label = k === 10 ? '50만' : `${k * 5}만~`;
      h += `<div class="histrow"><span class="num muted">${label}</span><i style="width:${maxCnt ? (cnt / maxCnt) * 100 : 0}%"></i><span class="num">${cnt}</span></div>`;
    }
    h += '</div>';
    const rows = b.results.slice().sort((a, b2) => b2.score - a.score);
    h += '<div class="tblwrap"><table><thead><tr><th>시드</th><th class="num">점수</th><th class="num">줄</th><th class="num">손패</th><th>동시 제거 1·2·3·4·5</th><th class="num">⊙/⇄ 사용</th><th></th></tr></thead><tbody>';
    for (const r of rows.slice(0, 60)) h += rowHtml(r);
    h += '</tbody></table></div>';
    if (rows.length > 60) h += `<div class="small muted">상위 60판만 표시</div>`;
  }
  out.innerHTML = h;
  for (const btn of out.querySelectorAll<HTMLButtonElement>('[data-replay]')) {
    btn.onclick = () => {
      s.stopAll();
      const seedEl = document.getElementById('sim-seed') as HTMLInputElement | null;
      if (seedEl) seedEl.value = btn.dataset.replay!;
      s.newGame(+btn.dataset.replay!);
      window.scrollTo({ top: 0, behavior: 'smooth' });
    };
  }
}

function rowHtml(r: GameResult): string {
  return `<tr><td class="num">${r.seed}</td><td class="num">${fmt(r.score)}${r.capped ? ' <span class="tag">캡</span>' : ''}${r.truncated ? ' <span class="tag info">상한</span>' : ''}</td><td class="num">${r.lines}</td><td class="num">${r.hands}</td><td class="num">${r.clears.slice(1).join(' · ')}</td><td class="num">${r.dots_used}/${r.rerolls_used}</td><td><button class="small" data-replay="${r.seed}" title="이 시드를 시뮬레이터에서 다시 보기">다시 보기</button></td></tr>`;
}

export function initSim(): void {
  simulator.onChange = () => renderSim();
}
