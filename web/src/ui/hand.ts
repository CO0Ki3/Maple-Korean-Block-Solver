// 손패 슬롯과 조각 라이브러리.
import { app } from '../app';
import { store } from '../store';
import { esc, miniShape } from './mini';

export function renderHand(): void {
  const el = document.getElementById('hand')!;
  const g = app.game;
  let html = '';
  for (let i = 0; i < 3; i++) {
    const pid = g.hand[i];
    const p = pid === null ? null : app.piece(pid);
    const meta = store.state.slots[i];
    const cls = `slot${store.state.target === i ? ' target' : ''}${p ? '' : ' empty'}`;
    html += `<div class="${cls}" data-slot="${i}"><span class="lbl">슬롯 ${i + 1}</span>`;
    if (p) {
      html += miniShape(meta.cur ?? p.cells, 11);
      html += `<span class="small"><b>${esc(p.name)}</b> ${p.n}칸</span>`;
      html += `<div class="acts"><button data-clr="${i}">비우기</button>${g.rerolls > 0 ? `<button data-rr="${i}" title="게임에서 바꿔 뽑기를 쓴 뒤 누르세요">바꿔 뽑기 사용</button>` : ''}</div>`;
    } else {
      html += `<span class="small muted" style="margin:auto 0">비어 있음 · 라이브러리에서 선택</span>`;
    }
    html += '</div>';
  }
  el.innerHTML = html;
  document.getElementById('handInfo')!.textContent = `${app.handCount()}/3`;
  for (const d of el.querySelectorAll<HTMLElement>('[data-slot]')) {
    d.onclick = (e) => {
      if ((e.target as HTMLElement).tagName === 'BUTTON') return;
      store.state.target = +d.dataset.slot!;
      store.save();
      renderHand();
    };
  }
  for (const b of el.querySelectorAll<HTMLButtonElement>('[data-clr]')) b.onclick = () => app.clearSlot(+b.dataset.clr!);
  for (const b of el.querySelectorAll<HTMLButtonElement>('[data-rr]')) b.onclick = () => app.useReroll(+b.dataset.rr!);
}

export function renderLib(): void {
  const el = document.getElementById('lib')!;
  const probs = store.probs();
  const counts = store.state.settings.counts;
  let html = '';
  for (const p of app.pieces) {
    const pr = probs[p.id];
    html += `<div class="pc" data-pid="${p.id}" tabindex="0" title="${esc(p.name)} ${p.n}칸 · 방향 ${p.orients.length}가지">
      <div class="shape">${miniShape(p.cells, 8)}</div>
      <div class="meta"><b>${esc(p.name)}</b> ${p.n}칸<br><span class="num">${counts[p.id]}회 · ${(pr * 100).toFixed(1)}%</span></div>
      <div class="bar"><span style="width:${Math.min(100, pr * 100 * 3)}%"></span></div>
      <div class="ctl"><button data-cnt="${p.id}" data-d="-1" aria-label="횟수 감소">−</button><button data-cnt="${p.id}" data-d="1" aria-label="횟수 증가">+</button></div>
    </div>`;
  }
  el.innerHTML = html;
  const tot = counts.reduce((a, b) => a + b, 0);
  document.getElementById('libInfo')!.textContent = `기록된 조각 ${tot}개`;
  for (const d of el.querySelectorAll<HTMLElement>('.pc')) {
    const pid = +d.dataset.pid!;
    d.onclick = (e) => {
      if ((e.target as HTMLElement).tagName === 'BUTTON') return;
      app.assignPiece(pid);
    };
    d.onkeydown = (e) => {
      if (e.key === 'Enter') app.assignPiece(pid);
    };
  }
  for (const b of el.querySelectorAll<HTMLButtonElement>('[data-cnt]')) {
    b.onclick = () => {
      const pid = +b.dataset.cnt!;
      const d = +b.dataset.d!;
      store.state.settings.counts[pid] = Math.max(0, store.state.settings.counts[pid] + d);
      store.save();
      renderLib();
    };
  }
}
