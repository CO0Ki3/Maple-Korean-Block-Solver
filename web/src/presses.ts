// 화면에 보이는 조각 모양(cur)에서 목표 방향(target)까지 회전·반전 버튼을 몇 번 눌러야 하는지.
import type { Cell } from './types';

export type RotDir = 'cw' | 'ccw';
export type FlipAxis = 'h' | 'v';

export interface Presses {
  flip: 0 | 1;
  rot: number;
  order: '' | 'fr' | 'rf';
  n: number;
}

export function normCells(cells: Cell[]): Cell[] {
  let mr = Infinity;
  let mc = Infinity;
  for (const [r, c] of cells) {
    mr = Math.min(mr, r);
    mc = Math.min(mc, c);
  }
  return cells.map(([r, c]) => [r - mr, c - mc] as Cell).sort((a, b) => a[0] - b[0] || a[1] - b[1]);
}

export function cellKey(cells: Cell[]): string {
  return JSON.stringify(normCells(cells));
}

export function rotate(cells: Cell[], dir: RotDir): Cell[] {
  return cells.map(([r, c]) => (dir === 'cw' ? ([c, -r] as Cell) : ([-c, r] as Cell)));
}

export function flip(cells: Cell[], axis: FlipAxis): Cell[] {
  return cells.map(([r, c]) => (axis === 'h' ? ([r, -c] as Cell) : ([-r, c] as Cell)));
}

/** 가장 적게 누르는 조합. 회전은 설정한 방향으로만 센다. */
export function pressesFor(cur: Cell[], target: Cell[], rotDir: RotDir, flipAxis: FlipAxis): Presses | null {
  const tk = cellKey(target);
  let best: Presses | null = null;
  const consider = (p: Presses) => {
    if (!best || p.n < best.n) best = p;
  };
  let x = cur;
  for (let k = 0; k < 4; k++) {
    if (cellKey(x) === tk) consider({ flip: 0, rot: k, order: '', n: k });
    x = rotate(x, rotDir);
  }
  x = flip(cur, flipAxis);
  for (let k = 0; k < 4; k++) {
    if (cellKey(x) === tk) consider({ flip: 1, rot: k, order: 'fr', n: k + 1 });
    x = rotate(x, rotDir);
  }
  x = cur;
  for (let k = 0; k < 4; k++) {
    if (cellKey(flip(x, flipAxis)) === tk) consider({ flip: 1, rot: k, order: 'rf', n: k + 1 });
    x = rotate(x, rotDir);
  }
  return best;
}

export function pressText(p: Presses | null): string {
  if (!p) return '';
  if (!p.n) return '돌리지 않고 그대로';
  const r = p.rot ? `회전 ${p.rot}번` : '';
  const f = '반전 1번';
  if (!p.flip) return r;
  if (!p.rot) return f;
  return p.order === 'fr' ? `${f} → ${r}` : `${r} → ${f}`;
}

/** 회전·반전을 모두 고려해 같은 조각인지 판별하는 정규형 키. */
export function canonKey(cells: Cell[]): string {
  const keys: string[] = [];
  let base = cells;
  for (const fl of [false, true]) {
    let x = fl ? flip(base, 'h') : base;
    for (let k = 0; k < 4; k++) {
      keys.push(cellKey(x));
      x = rotate(x, 'cw');
    }
  }
  base = cells;
  keys.sort();
  return keys[0];
}
