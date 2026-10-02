import type { BoardDetection, CellInfo, Prepped, Rect } from './types';
import { CLS_TEAL, CLS_WHITE } from './prep';

/** fitAxis 결과: 격자 한 축의 원점(x0)·간격(p)과 선 위/칸 가운데의 평균 점수. */
export interface AxisFit {
  p: number;
  x0: number;
  /** 격자선 위치(n+1개)의 평균 점수 */
  lineMean: number;
  /** 칸 가운데(n개)의 평균 점수 */
  midMean: number;
}

/**
 * 한 축(열 또는 행)의 점수 배열에 등간격 격자를 맞춘다.
 * score: 위치별 격자선 후보 수, n: 칸 수(선은 n+1개), len: 축 길이(px).
 * 간격 p는 6px부터 0.25px 단위로 훑고, 최적 근처를 0.02px / 0.25px 단위로 다시 다듬는다.
 */
export function fitAxis(score: ArrayLike<number>, n: number, len: number): AxisFit {
  const s2 = new Float32Array(len);
  for (let i = 0; i < len; i++) {
    const a = score[i];
    const b = i ? score[i - 1] : 0;
    const c = i < len - 1 ? score[i + 1] : 0;
    s2[i] = Math.max(a, b, c);
  }
  const at = (x: number): number => {
    const k = Math.round(x);
    return k >= 0 && k < len ? s2[k] : 0;
  };
  const tot = (x0: number, p: number): number => {
    let v = 0;
    for (let k = 0; k <= n; k++) v += at(x0 + k * p);
    return v;
  };
  let best = { v: -1, p: 0, x0: 0 };
  const pmax = (len - 2) / n;
  for (let p = 6; p <= pmax; p += 0.25) {
    for (let x0 = 0; x0 + n * p < len - 1; x0++) {
      const v = tot(x0, p);
      if (v > best.v) best = { v, p, x0 };
    }
  }
  if (best.v <= 0) return { p: 0, x0: 0, lineMean: 0, midMean: 0 };
  let b2 = best;
  for (let p2 = best.p - 0.3; p2 <= best.p + 0.3; p2 += 0.02) {
    for (let x1 = best.x0 - 1.5; x1 <= best.x0 + 1.5; x1 += 0.25) {
      if (x1 < 0 || x1 + n * p2 >= len - 1) continue;
      const v2 = tot(x1, p2);
      if (v2 > b2.v) b2 = { v: v2, p: p2, x0: x1 };
    }
  }
  let lm = 0;
  let mm = 0;
  let k: number;
  for (k = 0; k <= n; k++) lm += at(b2.x0 + k * b2.p);
  for (k = 0; k < n; k++) mm += at(b2.x0 + (k + 0.5) * b2.p);
  return { p: b2.p, x0: b2.x0, lineMean: lm / (n + 1), midMean: mm / n };
}

/**
 * 판 격자 검출. W = 가로 칸 수, H = 세로 칸 수.
 * 격자선 후보: 청록 픽셀이면서, 좌우 ±2px(또는 ±3px) 이웃이 둘 다 청록이고 둘 다 자기보다 DL(=6) 이상 밝은 점
 * (청록 칸 사이의 어두운 선). 상하 방향도 같은 방식으로 센다.
 * 열별·행별로 센 개수에 fitAxis로 등간격 격자를 맞춘다.
 */
export function detectBoard(P: Prepped, W: number, H: number): BoardDetection {
  const w = P.w;
  const h = P.h;
  const lum = P.lum;
  const cls = P.cls;
  const col = new Float32Array(w);
  const row = new Float32Array(h);
  const DL = 6;
  for (let y = 3; y < h - 3; y++) {
    const o = y * w;
    for (let x = 3; x < w - 3; x++) {
      const i = o + x;
      if (!(cls[i] & CLS_TEAL)) continue;
      const L = lum[i] + DL;
      if (
        ((cls[i - 2] & cls[i + 2] & CLS_TEAL) !== 0 && L <= lum[i - 2] && L <= lum[i + 2]) ||
        ((cls[i - 3] & cls[i + 3] & CLS_TEAL) !== 0 && L <= lum[i - 3] && L <= lum[i + 3])
      ) {
        col[x]++;
      }
      const a = i - 2 * w;
      const b = i + 2 * w;
      const a3 = i - 3 * w;
      const b3 = i + 3 * w;
      if (
        ((cls[a] & cls[b] & CLS_TEAL) !== 0 && L <= lum[a] && L <= lum[b]) ||
        ((cls[a3] & cls[b3] & CLS_TEAL) !== 0 && L <= lum[a3] && L <= lum[b3])
      ) {
        row[y]++;
      }
    }
  }
  const fx = fitAxis(col, W, w);
  const fy = fitAxis(row, H, h);
  const rect: Rect = { x0: fx.x0, y0: fy.x0, p: fx.p, q: fy.p };
  const vis = fx.p && fy.p ? Math.min(fx.lineMean / (H * fy.p), fy.lineMean / (W * fx.p)) : 0;
  const conf =
    vis >= 0.15 &&
    fx.lineMean > 2.5 * fx.midMean + 2 &&
    fy.lineMean > 2.5 * fy.midMean + 2 &&
    Math.abs(fx.p - fy.p) < 0.2 * Math.max(fx.p, fy.p);
  return { rect, conf, vis };
}

/** 중앙값. 길이가 짝수면 위쪽 값(a.length >> 1번째)을 쓴다. 빈 배열은 넘기지 말 것. */
export function median(a: number[]): number {
  const sorted = a.slice().sort((x, y) => x - y);
  return sorted[sorted.length >> 1];
}

/**
 * 판의 W×H 칸을 하나씩 측정해 채움/빈칸과 능력 아이콘을 판정한다.
 * 칸 안쪽 0.14~0.86 구간만 샘플링한다(격자선·가장자리 제외).
 */
export function classifyCells(P: Prepped, R: Rect, W: number, H: number): CellInfo[] {
  const out: CellInfo[] = [];
  const w = P.w;
  const h = P.h;
  const d = P.data;
  const cls = P.cls;
  const lum = P.lum;
  for (let r = 0; r < H; r++) {
    for (let c = 0; c < W; c++) {
      const xa = Math.max(0, Math.ceil(R.x0 + (c + 0.14) * R.p));
      const xb = Math.min(w - 1, Math.floor(R.x0 + (c + 0.86) * R.p));
      const ya = Math.max(0, Math.ceil(R.y0 + (r + 0.14) * R.q));
      const yb = Math.min(h - 1, Math.floor(R.y0 + (r + 0.86) * R.q));
      let n = 0;
      let t = 0;
      let wh = 0;
      let ed = 0;
      let sr = 0;
      let sg = 0;
      let sb = 0;
      let sl = 0;
      let sl2 = 0;
      let pur = 0;
      let nav = 0;
      for (let y = ya; y <= yb; y++) {
        for (let x = xa; x <= xb; x++) {
          const i = y * w + x;
          const k = i * 4;
          const R0 = d[k];
          const G0 = d[k + 1];
          const B0 = d[k + 2];
          const L = lum[i];
          n++;
          if (cls[i] & CLS_TEAL) t++;
          if (cls[i] & CLS_WHITE) wh++;
          sr += R0;
          sg += G0;
          sb += B0;
          sl += L;
          sl2 += L * L;
          if (x < xb && Math.abs(L - lum[i + 1]) > 6) ed++;
          const mx = Math.max(R0, G0, B0);
          const mn = Math.min(R0, G0, B0);
          const df = mx - mn;
          if (df > 0.3 * mx && mx > 90) {
            const hue =
              mx === R0
                ? 60 * (((G0 - B0) / df + 6) % 6)
                : mx === G0
                  ? 60 * ((B0 - R0) / df + 2)
                  : 60 * ((R0 - G0) / df + 4);
            if (hue >= 255 && hue <= 335) pur++;
            else if (hue >= 195 && hue <= 240 && mx < 180 && df > 0.45 * mx) nav++;
          }
        }
      }
      const mL = n ? sl / n : 0;
      out.push({
        r,
        c,
        teal: n ? t / n : 0,
        white: n ? wh / n : 0,
        edge: n ? ed / n : 0,
        std: n ? Math.sqrt(Math.max(0, sl2 / n - mL * mL)) : 0,
        m: n ? [sr / n, sg / n, sb / n] : [0, 0, 0],
        pur,
        nav,
        // 아래 판정 단계에서 채운다 (원본은 이 시점에 필드가 없고 나중에 붙인다)
        icon: false,
        state: 'empty',
      });
    }
  }
  /* 행마다 '빈칸 색' 기준: 평평한 청록 칸의 중앙값 (배경이 위아래로 그라데이션이라 행별로 잡음) */
  const ref: Array<[number, number, number] | null> = [];
  const known: number[] = [];
  for (let r2 = 0; r2 < H; r2++) {
    // 이 행에서 평평한 청록 칸(teal >= 0.9, 밝기 편차 std < 5)만 모은다
    const arr = out.filter((o) => o.r === r2 && o.teal >= 0.9 && o.std < 5);
    ref[r2] = arr.length
      ? [median(arr.map((o) => o.m[0])), median(arr.map((o) => o.m[1])), median(arr.map((o) => o.m[2]))]
      : null;
    if (ref[r2]) known.push(r2);
  }
  // 기준이 없는 행은 가장 가까운 '기준이 있는' 행의 색을 쓴다 (거리가 같으면 윗행)
  for (let r2 = 0; r2 < H; r2++) {
    if (!ref[r2] && known.length) {
      let nr = known[0];
      known.forEach((k) => {
        if (Math.abs(k - r2) < Math.abs(nr - r2)) nr = k;
      });
      ref[r2] = ref[nr];
    }
  }
  out.forEach((o) => {
    const rf = ref[o.r];
    const dist = rf ? Math.hypot(o.m[0] - rf[0], o.m[1] - rf[1], o.m[2] - rf[2]) : 0;
    /* 능력 아이콘: 흰 테두리 빛 + 촘촘한 경계. 블럭은 광택(밝기 편차)이 크고, 빈칸은 평평함 */
    o.icon = o.white >= 0.08 && o.edge >= 0.25;
    if (o.icon) {
      o.state = o.teal >= 0.4 ? 'empty' : 'filled';
      o.ab = o.pur > o.nav && o.pur >= 6 ? 'r' : 'e';
    } else if (o.std >= 9 && (o.teal < 0.75 || dist > 20)) o.state = 'filled';
    else if (o.teal >= 0.75) o.state = dist > 45 ? 'filled' : 'empty';
    else o.state = o.teal < 0.35 ? 'filled' : 'empty';
  });
  return out;
}
