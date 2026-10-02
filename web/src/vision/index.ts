import { prepImage } from './prep';
import { classifyCells, detectBoard } from './board';
import { detectHand } from './hand';
import type { Calibration, Recognition, Rect } from './types';

export * from './types';
export * from './prep';
export * from './board';
export * from './hand';

/**
 * 인식에 넘기는 이미지의 긴 변 최대 길이(px).
 *
 * 원본(handleImage)은 이미지를 캔버스에 그릴 때 긴 변이 2400px을 넘으면 비율을 유지해 줄인 뒤 인식한다:
 *   sc = min(1, 2400 / max(iw, ih)),  cw = max(1, round(iw * sc)),  ch = max(1, round(ih * sc))
 * 이 모듈은 DOM/캔버스를 쓰지 않으므로 축소하지 않는다. 호출자(UI)가 recognize에 넘기기 전에
 * 이 크기 이하로 줄인 ImageData를 만들어야 한다. 보정값(cal)의 iw/ih도 줄인 뒤의 크기 기준이다.
 */
export const MAX_IMAGE_SIDE = 2400;

/**
 * 스크린샷 한 장을 인식한다 (원본 handleImage + applyShot 중 UI 상태를 건드리기 전까지의 동작).
 *
 * prepImage → detectBoard. 격자를 믿을 수 없고(!det.conf) 같은 크기(cal.iw === img.width,
 * cal.ih === img.height)의 보정값 cal이 있으면 그 판 위치를 쓴다(usedCal = true).
 * ok = det.conf || usedCal 이면 classifyCells와 detectHand까지 실행하고,
 * 아니면 cells: [] / hand: { cards: [], m: 0 }으로 돌려준다 (이때 rect는 det.rect).
 *
 * W×H는 판의 가로·세로 칸 수. 인식에 성공하면 호출 측에서 cal = { iw: img.width, ih: img.height, ...rect }를
 * 저장해 두면 된다 (원본 applyShot과 같음).
 */
export function recognize(img: ImageData, W: number, H: number, cal: Calibration | null): Recognition {
  const P = prepImage(img);
  const det = detectBoard(P, W, H);
  let rect: Rect = det.rect;
  let usedCal = false;
  if (!det.conf && cal && cal.iw === img.width && cal.ih === img.height) {
    rect = { x0: cal.x0, y0: cal.y0, p: cal.p, q: cal.q };
    usedCal = true;
  }
  const ok = det.conf || usedCal;
  if (!ok) return { rect, ok, usedCal, det, cells: [], hand: { cards: [], m: 0 } };
  return { rect, ok, usedCal, det, cells: classifyCells(P, rect, W, H), hand: detectHand(P, rect, W, H) };
}

/**
 * 직접 지정한 판 위치로 인식한다 (수동 두 모서리 지정 경로). 자동 검출은 건너뛴다.
 *
 * ok = true, usedCal = false. 자동 검출 결과가 없으므로 det에는 { rect, conf: false, vis: 0 }을 넣는다
 * (ok이면서 usedCal도 det.conf도 false이면 수동 지정으로 구분할 수 있다).
 * 두 모서리 a(왼쪽 위), c(오른쪽 아래)로 rect를 만드는 식은 원본과 같다:
 *   { x0: a.x, y0: a.y, p: (c.x - a.x) / W, q: (c.y - a.y) / H }
 * 원본은 c.x - a.x < W * 3 또는 c.y - a.y < H * 3 이면 실패로 처리한다. 이 검사는 호출 측 몫이다.
 */
export function recognizeWithRect(img: ImageData, W: number, H: number, rect: Rect): Recognition {
  const P = prepImage(img);
  return {
    rect,
    ok: true,
    usedCal: false,
    det: { rect, conf: false, vis: 0 },
    cells: classifyCells(P, rect, W, H),
    hand: detectHand(P, rect, W, H),
  };
}
