// 범용 10×16 격자 HTML (솔버 판과 시뮬레이터 판이 같은 모양으로 그린다).
import type { Icon } from '../types';
import { H, W } from '../types';

export interface GridOpts {
  /** 칸 → 추가 클래스 (예: 'p1', 'er'). */
  hl?: Map<string, string>;
  /** 테두리로 표시할 행 (제거 예정). */
  rows?: Set<number>;
  /** 아이콘 순번 배지 표시. */
  badges?: boolean;
}

export function gridHtml(board: string[], icons: Icon[], opts: GridOpts = {}): string {
  const iconAt = new Map<string, { kind: string; order: number }>();
  icons.forEach((ic, i) => iconAt.set(`${ic.r},${ic.c}`, { kind: ic.kind, order: i + 1 }));
  let html = '';
  for (let r = 0; r < H; r++) {
    for (let c = 0; c < W; c++) {
      let cls = 'cell';
      if (board[r][c] === '#') cls += ' f';
      const ic = iconAt.get(`${r},${c}`);
      if (ic) cls += ic.kind === 'Reroll' ? ' ab-r' : ' ab-e';
      const extra = opts.hl?.get(`${r},${c}`);
      if (extra) cls += ` ${extra}`;
      if (opts.rows?.has(r)) cls += ' clr';
      const badge = ic ? `<b class="ic">${ic.kind === 'Reroll' ? '⇄' : '⊙'}${opts.badges === false ? '' : `<sub>${ic.order}</sub>`}</b>` : '';
      html += `<div class="${cls}" data-r="${r}" data-c="${c}">${badge}</div>`;
    }
  }
  return html;
}
