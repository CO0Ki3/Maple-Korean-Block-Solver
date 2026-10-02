import type { Prepped } from './types';

// cls[i] 비트 플래그 (원본의 cls[i]&1, &2, &4)
/** 청록(빈칸 배경색): 채도 0.28 이상, 밝기 0.45 이상, 색상(hue) 160~222도 */
export const CLS_TEAL = 1;
/** 흰색(손패 카드, 능력 아이콘의 흰 테두리): 채도 0.12 미만, 밝기 0.86 초과 */
export const CLS_WHITE = 2;
/** 채도 있는 색(블럭 색): 채도 0.25 초과, 밝기 0.35 초과 */
export const CLS_COLOR = 4;
/** 연노랑(선택된 손패 카드의 배경과 테두리): 색상 38~65도, 채도 0.12~0.72, 밝기 0.85 이상. 판의 노란 블럭(채도 ≈0.9)은 제외된다.
 *  원본에는 없던 비트. 손패 카드 탐색에만 쓰고, 칸 분류(아이콘 판정의 '흰색' 비율)에는 쓰지 않는다. */
export const CLS_PALE = 8;

/**
 * 이미지 전처리: 픽셀마다 밝기(lum)와 색 분류 비트(cls)를 계산한다.
 * 입력 img.data(RGBA)는 복사하지 않고 결과의 data로 그대로 넘긴다.
 */
export function prepImage(img: ImageData): Prepped {
  const w = img.width;
  const h = img.height;
  const d = img.data;
  const N = w * h;
  const lum = new Uint8Array(N);
  const cls = new Uint8Array(N);
  for (let i = 0, j = 0; i < N; i++, j += 4) {
    const r = d[j];
    const g = d[j + 1];
    const b = d[j + 2];
    const mx = r > g ? (r > b ? r : b) : g > b ? g : b;
    const mn = r < g ? (r < b ? r : b) : g < b ? g : b;
    const df = mx - mn;
    lum[i] = (r * 77 + g * 150 + b * 29) >> 8;
    const s = mx ? df / mx : 0;
    const v = mx / 255;
    let hue = 0;
    if (df) {
      if (mx === r) hue = 60 * (((g - b) / df + 6) % 6);
      else if (mx === g) hue = 60 * ((b - r) / df + 2);
      else hue = 60 * ((r - g) / df + 4);
    }
    let c = 0;
    if (s >= 0.28 && v >= 0.45 && hue >= 160 && hue <= 222) c |= CLS_TEAL;
    if (s < 0.12 && v > 0.86) c |= CLS_WHITE;
    if (s > 0.25 && v > 0.35) c |= CLS_COLOR;
    if (hue >= 38 && hue <= 65 && s >= 0.12 && s <= 0.72 && v >= 0.85) c |= CLS_PALE;
    cls[i] = c;
  }
  return { w, h, data: d, lum, cls };
}
