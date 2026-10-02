// 워커 풀: 탐색/바꿔 뽑기 분석을 메인 스레드 밖에서 병렬로 돌린다.
import type { WorkerRequest, WorkerResponse } from './worker';
import type { GameState, Plan, PlayConfig, RerollOption, RerollReport, SearchParams, SimGamesResult, SimRng, SimStepResult } from '../types';

interface Pending {
  resolve: (s: string) => void;
  reject: (e: Error) => void;
  req: WorkerRequest;
  gen: number;
}

export class WorkerPool {
  private workers: Worker[] = [];
  private busy: boolean[] = [];
  private queue: Pending[] = [];
  private inflight = new Map<number, Pending>();
  private nextId = 1;
  /** 세대 번호. 상태가 바뀌면 올려서 이전 요청 결과를 무시한다. */
  gen = 0;

  constructor(private size: number) {
    this.spawn();
  }

  private spawn(): void {
    for (const w of this.workers) w.terminate();
    this.workers = [];
    this.busy = [];
    for (let i = 0; i < this.size; i++) {
      const w = new Worker(new URL('./worker.ts', import.meta.url), { type: 'module' });
      w.onmessage = (e: MessageEvent<WorkerResponse>) => this.onDone(i, e.data);
      w.onerror = (e) => {
        console.error('worker error', e);
      };
      this.workers.push(w);
      this.busy.push(false);
    }
  }

  get active(): number {
    return this.inflight.size + this.queue.length;
  }

  /** 대기 중인 작업을 버린다. 실행 중인 wasm 호출은 중단할 수 없으므로 그때만 워커를 새로 만든다. */
  cancelAll(): void {
    this.gen++;
    for (const p of this.queue) p.reject(new Error('cancelled'));
    this.queue = [];
    if (this.inflight.size === 0) return;
    for (const p of this.inflight.values()) p.reject(new Error('cancelled'));
    this.inflight.clear();
    this.spawn();
  }

  private onDone(i: number, res: WorkerResponse): void {
    this.busy[i] = false;
    const p = this.inflight.get(res.id);
    this.inflight.delete(res.id);
    if (p) {
      if (res.ok && res.result !== undefined) p.resolve(res.result);
      else p.reject(new Error(res.error ?? 'worker failed'));
    }
    this.pump();
  }

  private pump(): void {
    for (let i = 0; i < this.workers.length && this.queue.length; i++) {
      if (this.busy[i]) continue;
      const p = this.queue.shift()!;
      if (p.gen !== this.gen) {
        p.reject(new Error('cancelled'));
        i--;
        continue;
      }
      this.busy[i] = true;
      this.inflight.set(p.req.id, p);
      this.workers[i].postMessage(p.req);
    }
  }

  private run(req: Omit<WorkerRequest, 'id'>): Promise<string> {
    return new Promise((resolve, reject) => {
      const full: WorkerRequest = { ...req, id: this.nextId++ };
      this.queue.push({ resolve, reject, req: full, gen: this.gen });
      this.pump();
    });
  }

  private pack(state: GameState, params: Partial<SearchParams>, weights: Record<string, number> | null, pw: number[] | null) {
    return {
      state: JSON.stringify(state),
      params: JSON.stringify(params),
      weights: weights ? JSON.stringify(weights) : '',
      pw: pw ? JSON.stringify(pw) : '',
    };
  }

  async solve(state: GameState, params: Partial<SearchParams>, weights: Record<string, number> | null, pw: number[] | null): Promise<Plan> {
    const s = await this.run({ op: 'solve', ...this.pack(state, params, weights, pw) });
    return JSON.parse(s) as Plan;
  }

  /** 리프 상태의 샘플링 lookahead 값. */
  async lookahead(leaf: GameState, params: Partial<SearchParams>, weights: Record<string, number> | null, pw: number[] | null, seed: number): Promise<number> {
    const s = await this.run({ op: 'lookahead', seed, ...this.pack(leaf, params, weights, pw) });
    return (JSON.parse(s) as { value: number }).value;
  }

  /** 시뮬레이터 정책 한 수. */
  async simStep(state: GameState, rng: SimRng, params: Partial<SearchParams>, weights: Record<string, number> | null, pw: number[] | null, cfg: PlayConfig): Promise<SimStepResult> {
    const s = await this.run({ op: 'simStep', rng: JSON.stringify(rng), cfg: JSON.stringify(cfg), ...this.pack(state, params, weights, pw) });
    return JSON.parse(s) as SimStepResult;
  }

  /** 여러 판을 끝까지 둔 통계 (한 워커에서 순차). */
  async simGames(games: number, seed: number, params: Partial<SearchParams>, weights: Record<string, number> | null, pw: number[] | null, cfg: PlayConfig): Promise<SimGamesResult> {
    const packed = this.pack({ board: [], hand: [], dots: 0, rerolls: 0, icons: [], placements: 0, lines: 0, score: 0, icon_seq: 0, over: false } as GameState, params, weights, pw);
    const s = await this.run({ op: 'simGames', games, seed, cfg: JSON.stringify(cfg), ...packed });
    return JSON.parse(s) as SimGamesResult;
  }

  /** 슬롯별로 워커를 나눠 돌린다. */
  async reroll(state: GameState, params: Partial<SearchParams>, weights: Record<string, number> | null, pw: number[] | null, basePlan: Plan | null): Promise<RerollReport> {
    const packed = this.pack(state, params, weights, pw);
    const slots = state.hand.map((h, i) => (h === null ? -1 : i)).filter((i) => i >= 0);
    const base = basePlan ?? (await this.solve(state, params, weights, pw));
    const opts = await Promise.all(slots.map((slot) => this.run({ op: 'rerollSlot', slot, ...packed }).then((s) => JSON.parse(s) as RerollOption | null)));
    return { base: base.value, base_complete: base.complete, options: opts.filter((o): o is RerollOption => o !== null) };
  }
}
