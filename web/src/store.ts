// 앱 상태, 저장(localStorage), 되돌리기.
import type { Cell, GameState, Plan, SearchParams } from './types';
import { H, NUM_PIECES, W } from './types';
import type { FlipAxis, RotDir } from './presses';

export type Mode = 'paint' | 'icon' | 'dot';

export interface Calibration {
  iw: number;
  ih: number;
  x0: number;
  y0: number;
  p: number;
  q: number;
}

export interface Settings {
  params: Partial<SearchParams>;
  workers: number;
  rotDir: RotDir;
  flipAxis: FlipAxis;
  /** null이면 엔진 기본 가중치. */
  weights: Record<string, number> | null;
  weightsLabel: string;
  /** 조각별 관측 횟수. */
  counts: number[];
  /** 라플라스 보정. */
  alpha: number;
  /** 손패 3개가 차면 자동으로 계산. */
  autoSolve: boolean;
  /** 설정 스키마 버전 (기본값 변경 시 마이그레이션용). */
  version: number;
}

export const SETTINGS_VERSION = 2;

export interface SlotMeta {
  /** 화면에 보이는 방향의 칸 목록 (회전·반전 안내용). null이면 기준 모양. */
  cur: Cell[] | null;
  /** 등장 횟수에 이미 반영했는가. */
  counted: boolean;
}

export interface AppState {
  game: GameState;
  slots: SlotMeta[];
  settings: Settings;
  mode: Mode;
  target: number;
  cal: Calibration | null;
  bestScore: number;
  example: boolean;
}

export const KEY = 'moamoa-v2';

export function blankGame(): GameState {
  return {
    board: Array.from({ length: H }, () => '.'.repeat(W)),
    hand: [null, null, null],
    dots: 0,
    rerolls: 0,
    icons: [],
    placements: 0,
    lines: 0,
    score: 0,
    icon_seq: 0,
    over: false,
  };
}

export function defaultSettings(): Settings {
  return {
    // 자기대전 비교: 빔 32 단독 평균 32.8만 → 다음 손패 샘플 4개 44.5만 (24판). 샘플링이 기본.
    params: { beam: 64, max_dots: 2, leaf_k: 48, samples: 4, sample_beam: 12, alts: 3 },
    workers: Math.max(2, Math.min(8, (navigator.hardwareConcurrency || 4) - 1)),
    rotDir: 'cw',
    flipAxis: 'h',
    weights: null,
    weightsLabel: '기본',
    counts: Array.from({ length: NUM_PIECES }, () => 0),
    alpha: 1,
    autoSolve: true,
    version: SETTINGS_VERSION,
  };
}

export function exampleState(): AppState {
  const g = blankGame();
  const ex: Record<number, string> = { 15: '######.###', 14: '#####...##', 13: '##.......#', 12: '#.........' };
  for (const k of Object.keys(ex)) g.board[+k] = ex[+k];
  g.icons = [{ r: 14, c: 6, kind: 'Dot', seq: 0 }];
  g.icon_seq = 1;
  g.hand = [4, 17, 15]; // ㅁ ㅏ ㅡ
  return {
    game: g,
    slots: [emptySlot(), emptySlot(), emptySlot()],
    settings: defaultSettings(),
    mode: 'paint',
    target: 0,
    cal: null,
    bestScore: 0,
    example: true,
  };
}

export function emptySlot(): SlotMeta {
  return { cur: null, counted: false };
}

type Listener = () => void;

class Store {
  state: AppState;
  plan: Plan | null = null;
  preview = 0;
  private listeners: Listener[] = [];
  private undoStack: string[] = [];

  constructor() {
    this.state = this.load() ?? exampleState();
  }

  private load(): AppState | null {
    try {
      const raw = localStorage.getItem(KEY);
      if (!raw) return null;
      const s = JSON.parse(raw) as AppState;
      if (!s.game || !Array.isArray(s.game.board) || s.game.board.length !== H) return null;
      s.settings = { ...defaultSettings(), ...s.settings, params: { ...defaultSettings().params, ...(s.settings?.params ?? {}) } };
      if (!Array.isArray(s.settings.counts) || s.settings.counts.length !== NUM_PIECES) s.settings.counts = defaultSettings().counts;
      if (!s.settings.version || s.settings.version < 2) {
        // v2: 샘플링 lookahead 기본 켬 (평가에서 가장 큰 이득).
        s.settings.params.samples = Math.max(s.settings.params.samples ?? 0, 4);
        s.settings.params.beam = Math.max(s.settings.params.beam ?? 0, 64);
        s.settings.version = 2;
      }
      if (!s.slots || s.slots.length !== 3) s.slots = [emptySlot(), emptySlot(), emptySlot()];
      s.game.icons = s.game.icons ?? [];
      s.game.icon_seq = s.game.icon_seq ?? s.game.icons.length;
      s.mode = s.mode ?? 'paint';
      s.target = s.target ?? 0;
      return s;
    } catch {
      return null;
    }
  }

  save(): void {
    try {
      localStorage.setItem(KEY, JSON.stringify(this.state));
    } catch {
      /* 저장 실패는 무시 */
    }
  }

  snapshot(): void {
    this.undoStack.push(JSON.stringify(this.state));
    if (this.undoStack.length > 80) this.undoStack.shift();
  }

  canUndo(): boolean {
    return this.undoStack.length > 0;
  }

  undo(): boolean {
    const s = this.undoStack.pop();
    if (!s) return false;
    this.state = JSON.parse(s) as AppState;
    this.plan = null;
    this.preview = 0;
    this.save();
    this.notify();
    return true;
  }

  subscribe(fn: Listener): void {
    this.listeners.push(fn);
  }

  notify(): void {
    for (const fn of this.listeners) fn();
  }

  /** 조각 가중치 = 횟수 + 알파. */
  pieceWeights(): number[] {
    const a = this.state.settings.alpha;
    return this.state.settings.counts.map((c) => c + a);
  }

  probs(): number[] {
    const w = this.pieceWeights();
    const t = w.reduce((x, y) => x + y, 0) || 1;
    return w.map((x) => x / t);
  }
}

export const store = new Store();
