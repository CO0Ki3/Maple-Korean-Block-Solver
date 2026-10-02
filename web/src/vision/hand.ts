import type { HandCard, HandDetection, Prepped, Rect } from './types';
import { CLS_COLOR, CLS_PALE, CLS_WHITE } from './prep';

/** 카드 배경: 흰색(일반 카드) 또는 연노랑(선택된 카드). */
const CARD_BG = CLS_WHITE | CLS_PALE;
import { median } from './board';

/** 흰 영역(카드 후보) 연결 성분. 좌표는 샘플 격자(st 픽셀 간격) 단위. */
interface Comp {
  x0: number;
  y0: number;
  x1: number;
  y1: number;
  /** 성분에 속한 샘플 수 */
  n: number;
}

/** pb(블럭 경계 상자)가 확정된 카드 */
type PieceCard = HandCard & { pb: NonNullable<HandCard['pb']> };

const hasPb = (c: HandCard): c is PieceCard => c.pb !== undefined;

/** 칸 목록을 (최소 행, 최소 열)이 0이 되도록 평행이동하고 (행, 열) 순으로 정렬한다. */
function normCells(cells: Array<[number, number]>): Array<[number, number]> {
  let mr = 1e9;
  let mc = 1e9;
  cells.forEach((p) => {
    mr = Math.min(mr, p[0]);
    mc = Math.min(mc, p[1]);
  });
  return cells.map((p): [number, number] => [p[0] - mr, p[1] - mc]).sort((a, b) => a[0] - b[0] || a[1] - b[1]);
}

/**
 * 손패 검출. R은 판 격자 위치(칸 크기 cp = R.p의 기준), W×H는 판 칸 수.
 * 흰 카드를 연결 성분으로 찾아 크기·채움 비율·판과의 겹침으로 거르고,
 * 카드 안 블럭 색 픽셀의 경계 상자에서 블럭 한 칸의 크기(m)를 추정해 모양(cells)을 읽는다.
 */
export function detectHand(P: Prepped, R: Rect, W: number, H: number): HandDetection {
  const w = P.w;
  const h = P.h;
  const cls = P.cls;
  const cp = R.p;
  // 흰색 판정은 st픽셀 간격으로 성기게 샘플링한 격자에서 한다
  const st = Math.max(1, Math.round(cp / 9));
  const gw = Math.floor(w / st);
  const gh = Math.floor(h / st);
  const lab = new Uint8Array(gw * gh);
  const comps: Comp[] = [];
  const stack: number[] = [];
  for (let gy = 0; gy < gh; gy++) {
    for (let gx = 0; gx < gw; gx++) {
      const gi = gy * gw + gx;
      if (lab[gi] || !(cls[gy * st * w + gx * st] & CARD_BG)) continue;
      const c: Comp = { x0: gx, y0: gy, x1: gx, y1: gy, n: 0 };
      lab[gi] = 1;
      stack.push(gi);
      while (stack.length) {
        const q = stack.pop() as number;
        const qx = q % gw;
        const qy = (q - qx) / gw;
        c.n++;
        if (qx < c.x0) c.x0 = qx;
        if (qx > c.x1) c.x1 = qx;
        if (qy < c.y0) c.y0 = qy;
        if (qy > c.y1) c.y1 = qy;
        const nb = [qx > 0 ? q - 1 : -1, qx < gw - 1 ? q + 1 : -1, qy > 0 ? q - gw : -1, qy < gh - 1 ? q + gw : -1];
        for (let k = 0; k < 4; k++) {
          const u = nb[k];
          if (u < 0 || lab[u]) continue;
          const ux = u % gw;
          const uy = (u - ux) / gw;
          if (cls[uy * st * w + ux * st] & CARD_BG) {
            lab[u] = 1;
            stack.push(u);
          }
        }
      }
      comps.push(c);
    }
  }
  const bx0 = R.x0;
  const by0 = R.y0;
  const bx1 = R.x0 + W * R.p;
  const by1 = R.y0 + H * R.q;
  let cards: HandCard[] = [];
  comps.forEach((c) => {
    const x0 = c.x0 * st;
    const y0 = c.y0 * st;
    const x1 = (c.x1 + 1) * st;
    const y1 = (c.y1 + 1) * st;
    const bw = x1 - x0;
    const bh = y1 - y0;
    // 카드 크기: 폭 2.5~12칸, 높이 1.5~8칸(칸 = cp)
    if (bw < 2.5 * cp || bh < 1.5 * cp || bw > 12 * cp || bh > 8 * cp) return;
    // 흰색으로 꽉 찬 정도(채움 비율)가 0.55 미만이면 카드가 아님
    if ((c.n * st * st) / (bw * bh) < 0.55) return;
    // 판 영역과 겹치는 면적이 카드의 0.2를 넘으면 카드가 아님
    const ix = Math.max(0, Math.min(x1, bx1) - Math.max(x0, bx0));
    const iy = Math.max(0, Math.min(y1, by1) - Math.max(y0, by0));
    if (ix * iy > 0.2 * bw * bh) return;
    cards.push({ x0, y0, x1, y1, bw, bh });
  });
  // 블럭 픽셀: 채도 있는 색이면서 흰색이 아닌 픽셀
  const isPc = (x: number, y: number): boolean => {
    const v = cls[y * w + x];
    return (v & CLS_COLOR) !== 0 && (v & CARD_BG) === 0;
  };
  cards.forEach((cd) => {
    // 카드의 왼쪽 0.06~0.45, 위아래 0.1 안쪽만 본다
    const xa = Math.round(cd.x0 + 0.06 * cd.bw);
    const xb = Math.round(cd.x0 + 0.45 * cd.bw);
    const ya = Math.round(cd.y0 + 0.1 * cd.bh);
    const yb = Math.round(cd.y1 - 0.1 * cd.bh);
    const colN = new Int32Array(xb - xa + 1);
    const rowN = new Int32Array(yb - ya + 1);
    let n = 0;
    for (let y = ya; y <= yb; y++) {
      for (let x = xa; x <= xb; x++) {
        if (isPc(x, y)) {
          n++;
          colN[x - xa]++;
          rowN[y - ya]++;
        }
      }
    }
    if (n < Math.pow(0.2 * cp, 2)) return;
    // 열/행별 개수가 최댓값의 0.2 미만인 가장자리를 잘라 낸 [시작, 끝]
    const trim = (arr: Int32Array): [number, number] => {
      let mx = 0;
      let i: number;
      for (i = 0; i < arr.length; i++) mx = Math.max(mx, arr[i]);
      let lo = 0;
      let hi = arr.length - 1;
      while (lo < hi && arr[lo] < 0.2 * mx) lo++;
      while (hi > lo && arr[hi] < 0.2 * mx) hi--;
      return [lo, hi];
    };
    const tx = trim(colN);
    const ty = trim(rowN);
    cd.pb = { x0: xa + tx[0], y0: ya + ty[0], bw: tx[1] - tx[0] + 1, bh: ty[1] - ty[0] + 1 };
  });
  const withP = cards.filter(hasPb);
  if (!withP.length) return { cards: [], m: 0 };
  // 카드 크기(bw, bh)가 (블럭 상자가 있는 카드들의) 중앙값에서 25% 넘게 벗어난 카드는 버리고, 위→아래·왼→오른 순으로 최대 3장
  const mw = median(withP.map((c) => c.bw));
  const mh = median(withP.map((c) => c.bh));
  cards = cards.filter((c) => Math.abs(c.bw - mw) < 0.25 * mw && Math.abs(c.bh - mh) < 0.25 * mh);
  cards.sort((a, b) => (Math.abs(a.y0 - b.y0) > 0.5 * mh ? a.y0 - b.y0 : a.x0 - b.x0));
  cards = cards.slice(0, 3);
  const pcs = cards.filter(hasPb);
  // 블럭 한 칸 크기의 사전값: 판 칸 크기의 0.32배
  const prior = 0.32 * cp;
  // 블럭 상자를 nx×ny 칸으로 나눴을 때 칸별 블럭 픽셀 비율(안쪽 0.2~0.8 구간)
  const occ = (cd: PieceCard, nx: number, ny: number): number[] => {
    const pb = cd.pb;
    const cw = pb.bw / nx;
    const ch = pb.bh / ny;
    const res: number[] = [];
    for (let j = 0; j < ny; j++) {
      for (let i = 0; i < nx; i++) {
        const xa = Math.round(pb.x0 + (i + 0.2) * cw);
        const xb = Math.round(pb.x0 + (i + 0.8) * cw);
        const ya = Math.round(pb.y0 + (j + 0.2) * ch);
        const yb = Math.round(pb.y0 + (j + 0.8) * ch);
        let n = 0;
        let t = 0;
        for (let y = ya; y <= yb; y++) {
          for (let x = xa; x <= xb; x++) {
            n++;
            if (isPc(x, y)) t++;
          }
        }
        res.push(n ? t / n : 0);
      }
    }
    return res;
  };
  // 칸 크기 m일 때 블럭의 [가로, 세로] 칸 수 (1~7칸으로 제한)
  const dims = (cd: PieceCard, m: number): [number, number] => [
    Math.max(1, Math.min(7, Math.round(cd.pb.bw / m))),
    Math.max(1, Math.min(7, Math.round(cd.pb.bh / m))),
  ];
  // 칸 크기 후보 m의 오차: 상자 크기가 칸의 정수배에서 벗어난 정도 + 칸이 반쯤 찬 정도(1.5배) + 사전값과의 거리(0.8배)
  const evalM = (m: number): number => {
    let tot = 0;
    pcs.forEach((cd) => {
      const dm = dims(cd, m);
      const err = Math.abs(cd.pb.bw - dm[0] * m) / m + Math.abs(cd.pb.bh - dm[1] * m) / m;
      let cl = 0;
      occ(cd, dm[0], dm[1]).forEach((v) => {
        cl += Math.min(v, 1 - v);
      });
      tot += err + 1.5 * cl;
    });
    return tot + 0.8 * Math.abs(Math.log(m / prior));
  };
  const cands: number[] = [prior];
  pcs.forEach((cd) => {
    for (let n = 1; n <= 7; n++) {
      cands.push(cd.pb.bw / n);
      cands.push(cd.pb.bh / n);
    }
  });
  let bm = prior;
  let bv = 1e9;
  cands.forEach((m) => {
    if (m < 3) return;
    const v = evalM(m);
    if (v < bv) {
      bv = v;
      bm = m;
    }
  });
  pcs.forEach((cd) => {
    const dm = dims(cd, bm);
    const o = occ(cd, dm[0], dm[1]);
    const cells: Array<[number, number]> = [];
    for (let j = 0; j < dm[1]; j++) {
      for (let i = 0; i < dm[0]; i++) {
        if (o[j * dm[0] + i] >= 0.5) cells.push([j, i]);
      }
    }
    cd.cells = cells.length ? normCells(cells) : null;
    cd.nx = dm[0];
    cd.ny = dm[1];
  });
  return { cards, m: bm };
}
