// 추천 배치 패널과 바꿔 뽑기 분석 표시.
import { app } from '../app';
import { pressesFor, pressText } from '../presses';
import { store } from '../store';
import { esc, fmt, miniShape } from './mini';

const STEP_COLORS = ['var(--s1)', 'var(--s2)', 'var(--s3)', 'var(--s4)'];

export function renderPlan(): void {
  const panel = document.getElementById('planPanel')!;
  const plan = store.plan;
  const hasHand = app.handCount() > 0;
  panel.hidden = !plan && !hasHand && !app.busy;
  const scoreEl = document.getElementById('planScore')!;
  const msg = document.getElementById('planMsg')!;
  const steps = document.getElementById('steps')!;
  const b1 = document.getElementById('b-apply1') as HTMLButtonElement;
  const bAll = document.getElementById('b-applyAll') as HTMLButtonElement;
  const bR = document.getElementById('b-reroll') as HTMLButtonElement;
  const bS = document.getElementById('b-solve') as HTMLButtonElement;
  bR.disabled = app.busy > 0 || !hasHand;
  bS.disabled = app.busy > 0 || !hasHand;
  if (!plan) {
    scoreEl.textContent = app.busy ? '계산 중…' : '';
    msg.innerHTML = hasHand && !app.busy ? '<div class="banner soft">손패를 넣으면 자동으로 계산합니다. 안 되면 「다시 계산」을 누르세요.</div>' : '';
    steps.innerHTML = '';
    b1.disabled = true;
    bAll.disabled = true;
    renderReroll();
    return;
  }
  scoreEl.innerHTML = `이번 손패 +<b>${fmt(plan.gain)}</b>점 · 평가 ${fmt(plan.value)}`;
  let m = '';
  if (!plan.complete) {
    m += `<div class="banner warn">손패 ${app.handCount()}개를 모두 놓는 방법이 없습니다. 최대 ${plan.placed}개까지 놓는 순서를 보여줍니다.` +
      (app.game.rerolls > 0 ? ' 바꿔 뽑기 분석으로 어떤 조각을 바꿀지 확인하세요.' : '') +
      (app.game.dots > 0 ? ' 점 찍기로 자리를 만들 수도 있습니다.' : '') +
      (app.game.dots + app.game.rerolls === 0 ? ' 능력이 없으면 게임이 끝납니다.' : '') + '</div>';
  }
  msg.innerHTML = m;
  let h = '';
  app.frames.forEach((fr, i) => {
    const mv = fr.move;
    const col = mv.t === 'Dot' ? 'var(--serase)' : STEP_COLORS[Math.min(3, i)];
    const sel = i === store.preview ? ' sel' : '';
    const tags = (fr.rows.length ? `<span class="tag">${fr.rows.length}줄 제거 +${fmt(300 * fr.rows.length * fr.rows.length)}</span>` : '') +
      (fr.outcome?.pickups ? `<span class="tag warn">능력 +${fr.outcome.pickups}</span>` : '') +
      (fr.outcome?.spawn_due ? (fr.after.dots + fr.after.rerolls >= 7 ? '<span class="tag info">7번째 배치 (보유 7개: 생성 없음)</span>' : '<span class="tag info">7번째 배치: 아이콘 생성</span>') : '') +
      (fr.outcome?.icon_dropped ? '<span class="tag info">최고령 아이콘 소멸</span>' : '');
    if (mv.t === 'Dot') {
      h += `<div class="step${sel}" data-st="${i}"><span class="chip" style="background:${col}">${i + 1}</span><div class="txt"><b>점 찍기</b> · ${mv.r + 1}행 ${mv.c + 1}열에 한 칸${tags}</div></div>`;
    } else if (mv.t === 'Place') {
      const pid = fr.before.hand[mv.slot];
      if (pid === null || pid === undefined) return;
      const p = app.piece(pid);
      const o = p.orients[mv.orient];
      const meta = store.state.slots[mv.slot];
      const s = store.state.settings;
      const turn = pressText(pressesFor(meta.cur ?? p.cells, o.cells, s.rotDir, s.flipAxis));
      h += `<div class="step${sel}" data-st="${i}"><span class="chip" style="background:${col}">${i + 1}</span>${miniShape(o.cells, 9, col)}
        <div class="txt"><b>${esc(p.name)}</b> <span class="muted small">(슬롯 ${mv.slot + 1})</span>${turn ? ` · <span class="turn">${turn}</span>` : ''}
        <br><span class="small">왼쪽 위 기준 ${mv.r + 1}행 ${mv.c + 1}열</span>${tags}</div></div>`;
    }
  });
  if (plan.alts.length) {
    h += `<div class="small muted">다른 후보: ${plan.alts.map((a, k) => `<button data-alt="${k}">후보 ${k + 2} (${fmt(a.value)})</button>`).join(' ')}</div>`;
  }
  steps.innerHTML = h;
  for (const d of steps.querySelectorAll<HTMLElement>('[data-st]')) {
    d.onclick = () => {
      store.preview = +d.dataset.st!;
      app.renderAll();
    };
  }
  for (const b of steps.querySelectorAll<HTMLButtonElement>('[data-alt]')) {
    b.onclick = () => {
      const k = +b.dataset.alt!;
      const a = plan.alts[k];
      plan.alts.splice(k, 1);
      plan.alts.unshift({ value: plan.value, gain: plan.gain, moves: plan.moves });
      plan.moves = a.moves;
      plan.value = a.value;
      plan.gain = a.gain;
      store.preview = 0;
      app.simulatePlan();
      app.renderAll();
    };
  }
  b1.disabled = !app.frames.length;
  bAll.disabled = !app.frames.length;
  renderReroll();
}

function renderReroll(): void {
  const out = document.getElementById('rerollOut')!;
  const rep = app.rerollReport;
  if (!rep) {
    out.innerHTML = '';
    return;
  }
  const g = app.game;
  let best: (typeof rep.options)[number] | null = null;
  for (const o of rep.options) if (!best || o.ev > best.ev) best = o;
  const gain = best ? best.ev - rep.base : 0;
  let h = `<div class="reroll"><p class="small" style="margin:6px 0">지금 손패 그대로의 평가: <b class="num">${fmt(rep.base)}</b>${rep.base_complete ? '' : ' (전부 놓을 수 없음)'}. 각 슬롯을 바꿔 뽑았을 때 조각 등장 확률로 가중한 기대 평가입니다 (바꿔 뽑기 1개 소모 포함).</p>`;
  h += '<table><thead><tr><th>바꿀 슬롯</th><th>조각</th><th class="num">기대 평가</th><th class="num">차이</th><th>못 놓는 확률</th></tr></thead><tbody>';
  for (const o of rep.options) {
    const pid = g.hand[o.slot];
    const d = o.ev - rep.base;
    const bad = o.per_piece.filter((x) => !x.complete).reduce((a, x) => a + x.prob, 0);
    h += `<tr><td>슬롯 ${o.slot + 1}</td><td>${pid === null ? '?' : esc(app.piece(pid).name)}</td><td class="num">${fmt(o.ev)}</td><td class="num" style="color:${d > 0 ? 'var(--accent)' : 'var(--muted)'}">${d > 0 ? '+' : ''}${fmt(d)}</td><td>${(bad * 100).toFixed(0)}%</td></tr>`;
  }
  h += '</tbody></table>';
  if (best && !rep.base_complete) {
    h += `<div class="banner warn">지금 손패는 다 놓을 수 없습니다. 바꿔 뽑기를 쓰지 않으면 게임이 끝나므로 슬롯 ${best.slot + 1}을 바꾸세요 (가장 유리한 선택).${g.rerolls > 0 ? '' : ' 보유한 바꿔 뽑기가 없다면 점 찍기를 쓰세요.'}</div>`;
  } else if (best && gain > 0) {
    h += `<div class="banner ok">추천: 슬롯 ${best.slot + 1} 바꿔 뽑기 (기대 +${fmt(gain)})${g.rerolls > 0 ? '' : ' · 보유한 바꿔 뽑기가 없습니다'}${g.dots + g.rerolls >= 7 ? ' · 보유가 가득 차 새 아이콘이 생기지 않으니 쓰는 편이 이득입니다' : ''}</div>`;
  } else {
    h += '<div class="banner soft">바꿔 뽑기를 아껴두는 편이 낫습니다. 기대 이득이 없습니다.</div>';
  }
  h += '</div>';
  out.innerHTML = h;
}

export function initPlan(): void {
  document.getElementById('b-apply1')!.onclick = () => app.applyOne();
  document.getElementById('b-applyAll')!.onclick = () => app.applyAll();
  document.getElementById('b-reroll')!.onclick = () => void app.rerollAnalysis();
  document.getElementById('b-solve')!.onclick = () => {
    app.invalidate();
    void app.solve();
  };
}
