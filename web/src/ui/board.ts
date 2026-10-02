// 게임판 렌더링과 클릭 처리.
import { app } from '../app';
import { normalizeState } from '../engine/wasm';
import { store } from '../store';
import { H, W } from '../types';

let painting: boolean | null = null;

export function renderBoard(): void {
  const el = document.getElementById('board')!;
  el.style.gridTemplateColumns = `repeat(${W}, 1fr)`;
  let board = app.game.board;
  let icons = app.game.icons;
  const hl = new Set<string>();
  const clr = new Set<number>();
  let stepCls = '';
  const fr = app.frames.length ? app.frames[Math.min(store.preview, app.frames.length - 1)] : null;
  if (fr) {
    board = fr.before.board;
    icons = fr.before.icons;
    const idx = app.frames.indexOf(fr);
    stepCls = fr.move.t === 'Dot' ? 'er' : `p${Math.min(4, idx + 1)}`;
    for (const [r, c] of fr.cells) hl.add(`${r},${c}`);
    for (const r of fr.rows) clr.add(r);
  }
  const iconAt = new Map<string, { kind: string; order: number }>();
  icons.forEach((ic, i) => iconAt.set(`${ic.r},${ic.c}`, { kind: ic.kind, order: i + 1 }));
  let html = '';
  for (let r = 0; r < H; r++) {
    for (let c = 0; c < W; c++) {
      let cls = 'cell';
      if (board[r][c] === '#') cls += ' f';
      const ic = iconAt.get(`${r},${c}`);
      if (ic) cls += ic.kind === 'Reroll' ? ' ab-r' : ' ab-e';
      if (hl.has(`${r},${c}`)) cls += ` ${stepCls}`;
      if (clr.has(r)) cls += ' clr';
      const badge = ic ? `<b class="ic">${ic.kind === 'Reroll' ? '⇄' : '⊙'}<sub>${ic.order}</sub></b>` : '';
      html += `<div class="${cls}" data-r="${r}" data-c="${c}" title="${r + 1}행 ${c + 1}열">${badge}</div>`;
    }
  }
  el.innerHTML = html;
  const info = document.getElementById('boardInfo')!;
  info.textContent = `${W}×${H} · 채움 ${app.boardFilled()}칸 · 빈 칸 ${app.emptyCells()}`;
  const note = document.getElementById('prevNote')!;
  if (fr) {
    const idx = app.frames.indexOf(fr) + 1;
    note.innerHTML = `<div class="banner soft">${idx}단계 미리보기: 색칠된 위치에 놓으세요${fr.rows.length ? ` · 테두리 줄 ${fr.rows.length}개 제거` : ''}</div>`;
  } else if (store.state.example) {
    note.innerHTML = '<div class="banner soft">예시 판과 손패입니다. 스크린샷을 붙여넣거나 실제 게임 화면대로 칸을 칠하세요.</div>';
  } else {
    note.innerHTML = '';
  }
  for (const b of document.querySelectorAll<HTMLButtonElement>('[data-mode]')) b.classList.toggle('on', b.dataset.mode === store.state.mode);
}

function cellAt(e: PointerEvent): HTMLElement | null {
  const t = document.elementFromPoint(e.clientX, e.clientY) as HTMLElement | null;
  return t && t.classList.contains('cell') ? t : null;
}

function touch(t: HTMLElement, start: boolean): void {
  const r = +t.dataset.r!;
  const c = +t.dataset.c!;
  const mode = store.state.mode;
  if (mode === 'paint') {
    if (start) {
      painting = app.game.board[r][c] !== '#';
      store.snapshot();
      store.state.example = false;
    }
    const want = painting === true;
    if ((app.game.board[r][c] === '#') !== want) {
      app.setCell(r, c, want);
      app.invalidate();
      app.renderAll();
    }
  } else if (mode === 'icon' && start) {
    app.cycleIcon(r, c);
  } else if (mode === 'dot' && start) {
    if (app.game.dots <= 0) {
      app.status('보유한 점 찍기가 없습니다.', 'warn');
      return;
    }
    app.useDot(r, c);
  }
}

export function initBoard(): void {
  const el = document.getElementById('board')!;
  el.addEventListener('pointerdown', (e) => {
    const t = cellAt(e);
    if (!t) return;
    e.preventDefault();
    touch(t, true);
    const move = (ev: PointerEvent) => {
      if (store.state.mode !== 'paint') return;
      const t2 = cellAt(ev);
      if (t2) touch(t2, false);
    };
    const up = () => {
      painting = null;
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', up);
      if (store.state.mode === 'paint') {
        // 칠한 뒤에는 종료 플래그를 엔진 기준으로 다시 판정한다.
        store.state.game.over = false;
        try {
          store.state.game = normalizeState(store.state.game);
        } catch {
          /* 상태가 깨졌으면 그대로 둔다 */
        }
        store.save();
        app.renderAll();
        app.scheduleSolve();
      }
    };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up);
  });
  for (const b of document.querySelectorAll<HTMLButtonElement>('[data-mode]')) {
    b.onclick = () => {
      store.state.mode = b.dataset.mode as typeof store.state.mode;
      store.save();
      renderBoard();
    };
  }
  document.getElementById('b-clear')!.onclick = () => {
    app.mutate(() => {
      const g = store.state.game;
      g.board = g.board.map(() => '.'.repeat(W));
      g.icons = [];
    });
  };
}
