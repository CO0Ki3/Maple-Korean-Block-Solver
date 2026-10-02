// 탐색 워커. 메시지 하나 = 작업 하나. 결과는 JSON 문자열 그대로 돌려준다.
import init, * as wasm from '../pkg/moamoa.js';

export interface WorkerRequest {
  id: number;
  op: 'solve' | 'reroll' | 'rerollSlot' | 'lookahead' | 'simStep' | 'simGames';
  state: string;
  params: string;
  weights: string;
  pw: string;
  slot?: number;
  seed?: number;
  rng?: string;
  cfg?: string;
  games?: number;
}

export interface WorkerResponse {
  id: number;
  ok: boolean;
  result?: string;
  error?: string;
  ms?: number;
}

const ctx = self as unknown as { onmessage: ((e: MessageEvent<WorkerRequest>) => void) | null; postMessage: (m: WorkerResponse) => void };
const ready = init();

ctx.onmessage = async (e: MessageEvent<WorkerRequest>) => {
  const m = e.data;
  const t0 = performance.now();
  try {
    await ready;
    let result: string;
    switch (m.op) {
      case 'solve':
        result = wasm.solve_json(m.state, m.params, m.weights, m.pw);
        break;
      case 'reroll':
        result = wasm.reroll_json(m.state, m.params, m.weights, m.pw);
        break;
      case 'rerollSlot':
        result = wasm.reroll_slot_json(m.state, m.params, m.weights, m.pw, m.slot ?? 0);
        break;
      case 'lookahead':
        result = wasm.lookahead_json(m.state, m.params, m.weights, m.pw, (m.seed ?? 1) >>> 0);
        break;
      case 'simStep':
        result = wasm.sim_step_json(m.state, m.rng ?? '', m.params, m.weights, m.pw, m.cfg ?? '');
        break;
      case 'simGames':
        result = wasm.sim_games_json((m.games ?? 1) >>> 0, (m.seed ?? 1) >>> 0, m.params, m.weights, m.pw, m.cfg ?? '');
        break;
      default:
        throw new Error('unknown op');
    }
    ctx.postMessage({ id: m.id, ok: true, result, ms: performance.now() - t0 });
  } catch (err) {
    ctx.postMessage({ id: m.id, ok: false, error: String(err), ms: performance.now() - t0 });
  }
};
