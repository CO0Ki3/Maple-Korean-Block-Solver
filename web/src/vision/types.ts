// 스크린샷 인식(vision) 모듈이 공유하는 타입.
// block-solver.html 원본이 쓰던 객체 모양을 그대로 옮겼다. 필드 이름은 바꾸지 않았다.

/** 판 격자의 위치. 이미지 픽셀 좌표 기준. */
export interface Rect {
  /** 격자 원점(왼쪽 위) x */
  x0: number;
  /** 격자 원점(왼쪽 위) y */
  y0: number;
  /** 칸 가로 간격(px) */
  p: number;
  /** 칸 세로 간격(px) */
  q: number;
}

/**
 * 저장해 둔 판 위치(보정값). 같은 크기(iw×ih)의 이미지에서만 다시 쓴다.
 * iw/ih는 축소를 마친 뒤(recognize에 넘기는) 이미지의 크기다.
 */
export interface Calibration extends Rect {
  iw: number;
  ih: number;
}

/** prepImage 결과. data는 입력 ImageData.data를 복사 없이 그대로 가리킨다. */
export interface Prepped {
  w: number;
  h: number;
  data: Uint8ClampedArray;
  /** 픽셀별 밝기: (r*77 + g*150 + b*29) >> 8 */
  lum: Uint8Array;
  /** 픽셀별 색 분류 비트마스크 (prep.ts의 CLS_TEAL | CLS_WHITE | CLS_COLOR) */
  cls: Uint8Array;
}

/** 칸 하나의 측정값과 판정 결과. */
export interface CellInfo {
  /** 행 / 열 (0부터) */
  r: number;
  c: number;
  /** 샘플 영역 중 청록 픽셀 비율 */
  teal: number;
  /** 흰색 픽셀 비율 */
  white: number;
  /** 가로로 이웃한 픽셀과 밝기 차가 6을 넘는 비율 (경계 밀도) */
  edge: number;
  /** 밝기 표준편차 */
  std: number;
  /** 평균 색 [R, G, B] */
  m: [number, number, number];
  /** 보라색 계열(hue 255~335) 픽셀 수 */
  pur: number;
  /** 남색 계열(hue 195~240, 어둡고 진한 색) 픽셀 수 */
  nav: number;
  /** 능력 아이콘이 있는 칸인가 */
  icon: boolean;
  state: 'filled' | 'empty';
  /** icon일 때만 있음: 'e' = 점찍기(⊙), 'r' = 바꿔뽑기(⇄) */
  ab?: 'e' | 'r';
}

/** 손패 카드 한 장(흰 카드 영역)과 그 안의 블럭. */
export interface HandCard {
  /** 카드 영역(이미지 픽셀). x1, y1은 오른쪽·아래 끝(미포함 경계). bw, bh는 폭·높이 */
  x0: number;
  y0: number;
  x1: number;
  y1: number;
  bw: number;
  bh: number;
  /** 카드 안 블럭 색 픽셀의 경계 상자. 블럭 픽셀이 너무 적으면 없음 */
  pb?: { x0: number; y0: number; bw: number; bh: number };
  /** 블럭 모양: 정규화된 [행, 열] 목록. 읽을 칸이 없으면 null. pb가 있는 카드에만 채워짐 */
  cells?: Array<[number, number]> | null;
  /** 블럭의 가로 / 세로 칸 수 */
  nx?: number;
  ny?: number;
}

export interface HandDetection {
  cards: HandCard[];
  /** 손패 블럭 한 칸의 픽셀 크기 추정값 (카드를 못 찾으면 0) */
  m: number;
}

export interface BoardDetection {
  rect: Rect;
  /** 격자를 믿을 수 있는가 */
  conf: boolean;
  /** 격자선 가시성 점수 (conf 판정에 쓰이는 vis) */
  vis: number;
}

export interface Recognition {
  rect: Rect;
  ok: boolean;
  usedCal: boolean;
  det: BoardDetection;
  cells: CellInfo[];
  hand: HandDetection;
}
