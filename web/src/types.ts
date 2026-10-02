// 엔진(Rust)의 serde JSON 표현과 1:1로 맞춘 타입들.

export type AbilityKind = 'Dot' | 'Reroll';

export interface Icon {
  r: number;
  c: number;
  kind: AbilityKind;
  seq: number;
}

export interface GameState {
  /** 16줄 × 10글자, '#' = 채움, '.' = 빈 칸. 0번이 맨 위. */
  board: string[];
  /** 3슬롯, null = 비어 있음(배치됨). */
  hand: (number | null)[];
  dots: number;
  rerolls: number;
  icons: Icon[];
  /** 누적 조각 배치 횟수 (점 찍기 제외). 7의 배수마다 아이콘 생성. */
  placements: number;
  lines: number;
  score: number;
  icon_seq: number;
  over: boolean;
}

export type Cell = [number, number];

export interface OrientInfo {
  h: number;
  w: number;
  rot: number;
  flip: boolean;
  cells: Cell[];
}

export interface PieceInfo {
  id: number;
  name: string;
  n: number;
  cells: Cell[];
  orients: OrientInfo[];
}

export type Move =
  | { t: 'Place'; slot: number; orient: number; r: number; c: number }
  | { t: 'Dot'; r: number; c: number }
  | { t: 'Reroll'; slot: number };

export interface Alt {
  value: number;
  gain: number;
  moves: Move[];
}

export interface Plan {
  moves: Move[];
  value: number;
  gain: number;
  complete: boolean;
  placed: number;
  alts: Alt[];
  leaves: number;
  nodes: number;
}

export interface PieceOutcome {
  piece: number;
  prob: number;
  value: number;
  complete: boolean;
}

export interface RerollOption {
  slot: number;
  ev: number;
  per_piece: PieceOutcome[];
}

export interface RerollReport {
  base: number;
  base_complete: boolean;
  options: RerollOption[];
}

export interface PlaceOutcome {
  cells: number;
  lines: number;
  pickups: number;
  cleared_mask?: number;
  granted?: Icon[];
  dropped?: Icon | null;
  spawn_due: boolean;
  hand_empty: boolean;
  icon_dropped: boolean;
  score_delta: number;
}

// ---- 시뮬레이터 (engine::sim) ----

export interface PlayConfig {
  max_hands?: number;
  replan_each_move?: boolean;
  reroll_margin?: number;
  reroll_at_cap?: boolean;
  reroll_beam?: number;
  replan_on_spawn?: boolean;
}

export type SimEvent =
  | { t: 'Plan'; complete: boolean; gain: number; value: number; moves: number }
  | { t: 'Reroll'; slot: number; old: number; new: number; ev: number; base: number; forced: boolean }
  | {
      t: 'Place';
      slot: number;
      piece: number;
      orient: number;
      r: number;
      c: number;
      cells: Cell[];
      cleared: number[];
      granted: Icon[];
      score_delta: number;
      spawned: Icon | null;
      dropped: Icon | null;
      spawn_blocked: boolean;
      new_hand: (number | null)[] | null;
    }
  | { t: 'Dot'; r: number; c: number; cleared: number[]; granted: Icon[]; score_delta: number; forced: boolean }
  | { t: 'GameOver'; reason: string };

export interface SimRng {
  s: string[];
}

export interface SimStepResult {
  state: GameState;
  rng: SimRng;
  events: SimEvent[];
}

export interface GameResult {
  seed: number;
  score: number;
  lines: number;
  hands: number;
  placements: number;
  clears: number[];
  pickups: number;
  dots_used: number;
  rerolls_used: number;
  capped: boolean;
  truncated: boolean;
}

export interface Summary {
  games: number;
  mean: number;
  median: number;
  min: number;
  max: number;
  mean_lines: number;
  mean_hands: number;
  clears: number[];
  multi_ratio: number;
  capped: number;
}

export interface SimGamesResult {
  results: GameResult[];
  summary: Summary;
}

export interface ApplyResult {
  state: GameState;
  outcome: PlaceOutcome | null;
  new_piece: number | null;
}

export interface SearchParams {
  beam: number;
  max_dots: number;
  leaf_k: number;
  samples: number;
  samples_top: number;
  sample_beam: number;
  alts: number;
  death_penalty: number;
  stuck_penalty: number;
}

export const W = 10;
export const H = 16;
export const NUM_PIECES = 19;
export const ABILITY_CAP = 7;
export const SPAWN_EVERY = 7;
