// 메인 스레드용 wasm 래퍼. 무거운 탐색은 worker.ts/pool.ts로 보낸다.
import init, * as wasm from '../pkg/moamoa.js';
import type { ApplyResult, GameResult, GameState, Move, PieceInfo, SearchParams, SimEvent, SimRng } from '../types';

let ready: Promise<void> | null = null;

export function ensureWasm(): Promise<void> {
  if (!ready) ready = init().then(() => undefined);
  return ready;
}

export function enginePieces(): PieceInfo[] {
  return JSON.parse(wasm.pieces_json()) as PieceInfo[];
}

export function defaultWeights(): Record<string, number> {
  return JSON.parse(wasm.default_weights_json()) as Record<string, number>;
}

export function featureNames(): string[] {
  return JSON.parse(wasm.feature_names_json()) as string[];
}

export function defaultParams(): SearchParams {
  return JSON.parse(wasm.default_params_json()) as SearchParams;
}

export function newGame(seed: number, pw: number[] | null): GameState {
  return JSON.parse(wasm.new_game_json(seed >>> 0, pw ? JSON.stringify(pw) : '')) as GameState;
}

/** 결정적 적용 (simulate=false): 아이콘 생성·새 손패는 하지 않는다. */
export function applyMove(state: GameState, mv: Move, simulate = false, seed = 0, pw: number[] | null = null): ApplyResult {
  const out = wasm.apply_move_json(JSON.stringify(state), JSON.stringify(mv), simulate, seed >>> 0, pw ? JSON.stringify(pw) : '');
  return JSON.parse(out) as ApplyResult;
}

export interface Evaluation {
  value: number;
  features: Array<[string, number, number]>;
  stuck: boolean;
  can_place_any: boolean;
}

export function evaluate(state: GameState, weights: Record<string, number> | null, pw: number[] | null): Evaluation {
  const out = wasm.evaluate_json(JSON.stringify(state), weights ? JSON.stringify(weights) : '', pw ? JSON.stringify(pw) : '');
  return JSON.parse(out) as Evaluation;
}

/** 시뮬레이터 새 게임: 손패 3개를 뽑은 상태와 난수 상태. */
export function simNew(seed: number, pw: number[] | null): { state: GameState; rng: SimRng } {
  return JSON.parse(wasm.sim_new_json(seed >>> 0, pw ? JSON.stringify(pw) : '')) as { state: GameState; rng: SimRng };
}

/** 이벤트를 통계에 반영 (엔진과 같은 집계 규칙). */
export function simRecord(result: GameResult, events: SimEvent[]): GameResult {
  return JSON.parse(wasm.sim_record_json(JSON.stringify(result), JSON.stringify(events))) as GameResult;
}

export function normalizeState(state: GameState): GameState {
  return JSON.parse(wasm.normalize_state_json(JSON.stringify(state))) as GameState;
}
