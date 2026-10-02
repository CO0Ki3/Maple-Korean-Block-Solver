// 시뮬레이터 컨트롤러: 규칙대로 손패가 들어오고, 솔버와 같은 정책(탐색 + 상황에 맞는 능력 사용)으로 자동 배치한다.
// 정책은 엔진의 `policy_step`(네이티브 자기대전과 동일)이며, 여기서는 호출·애니메이션·통계·로그만 맡는다.
import { app } from '../app';
import { applyMove, simNew, simRecord } from '../engine/wasm';
import { WorkerPool } from '../engine/pool';
import { store } from '../store';
import type { GameResult, GameState, Icon, PlayConfig, SearchParams, SimEvent, SimRng, Summary } from '../types';
import { ABILITY_CAP } from '../types';

export type Speed = 'slow' | 'normal' | 'fast' | 'max';
const DELAY: Record<Speed, number> = { slow: 700, normal: 320, fast: 110, max: 0 };

export interface AnimFrame {
  board: string[];
  icons: Icon[];
  hand: (number | null)[];
  hl: Map<string, string>;
  rows: Set<number>;
}

export interface LogEntry {
  step: number;
  text: string;
  kind: 'place' | 'clear' | 'ability' | 'reroll' | 'dot' | 'over' | 'info';
}

export interface SimSettings {
  beam: number;
  maxDots: number;
  /** 다음 손패 샘플 수 (0=끔). 켜면 수당 계산이 5~8배 느려지지만 점수가 크게 오른다. */
  samples: number;
  /** 손패 추첨에 기록된 등장 횟수(설정의 조각 가중치)를 쓸지. 끄면 19종 균등. */
  usePieceCounts: boolean;
  replanOnSpawn: boolean;
  rerollAtCap: boolean;
}

export interface BatchProgress {
  done: number;
  total: number;
  results: GameResult[];
  /** 실행 당시 설정 요약 (비교용). */
  label: string;
}

export function blankResult(seed: number): GameResult {
  return { seed, score: 0, lines: 0, hands: 0, placements: 0, clears: [0, 0, 0, 0, 0, 0], pickups: 0, dots_used: 0, rerolls_used: 0, capped: false, truncated: false };
}

export function summarize(results: GameResult[]): Summary {
  const n = results.length;
  if (!n) return { games: 0, mean: 0, median: 0, min: 0, max: 0, mean_lines: 0, mean_hands: 0, clears: [0, 0, 0, 0, 0, 0], multi_ratio: 0, capped: 0 };
  const scores = results.map((r) => r.score).sort((a, b) => a - b);
  const clears = [0, 0, 0, 0, 0, 0];
  for (const r of results) for (let k = 1; k < 6; k++) clears[k] += r.clears[k] ?? 0;
  const total = clears.slice(1).reduce((a, b) => a + b, 0);
  const multi = clears.slice(2).reduce((a, b) => a + b, 0);
  return {
    games: n,
    mean: scores.reduce((a, b) => a + b, 0) / n,
    median: n % 2 === 0 ? (scores[n / 2 - 1] + scores[n / 2]) / 2 : scores[(n - 1) / 2],
    min: scores[0],
    max: scores[n - 1],
    mean_lines: results.reduce((a, r) => a + r.lines, 0) / n,
    mean_hands: results.reduce((a, r) => a + r.hands, 0) / n,
    clears,
    multi_ratio: total ? multi / total : 0,
    capped: results.filter((r) => r.capped).length,
  };
}

const sleep = (ms: number) => (ms > 0 ? new Promise<void>((r) => setTimeout(r, ms)) : Promise.resolve());

export class Simulator {
  state: GameState | null = null;
  rng: SimRng | null = null;
  seed = 1;
  stats: GameResult = blankResult(1);
  log: LogEntry[] = [];
  frame: AnimFrame | null = null;
  running = false;
  busy = false;
  speed: Speed = 'normal';
  stepCount = 0;
  elapsedMs = 0;
  lastStepMs = 0;
  settings: SimSettings = { beam: 32, maxDots: 2, samples: 0, usePieceCounts: false, replanOnSpawn: true, rerollAtCap: true };
  batch: BatchProgress | null = null;
  batchRunning = false;
  /** 일괄 실행 중 왼쪽 화면에서 한 판씩 자동 관전 중인가. */
  spectating = false;
  onChange: () => void = () => {};
  private pool: WorkerPool | null = null;
  private batchPool: WorkerPool | null = null;
  /** 한 판 보기 세대 (새 게임·중단 시 증가). */
  private gen = 0;
  /** 일괄 실행 세대 (중단 시에만 증가). 관전용 새 게임이 일괄 실행 결과를 무효화하면 안 된다. */
  private batchGen = 0;

  /** 한 판 보기용 (워커 2개). 일괄 실행과 분리해 관전이 큐 뒤에 밀리지 않게 한다. */
  private getPool(): WorkerPool {
    if (!this.pool) this.pool = new WorkerPool(2);
    return this.pool;
  }

  /** 일괄 실행용 (설정의 워커 수). */
  private getBatchPool(): WorkerPool {
    if (!this.batchPool) this.batchPool = new WorkerPool(Math.max(1, store.state.settings.workers));
    return this.batchPool;
  }

  private params(): Partial<SearchParams> {
    const samples = Math.max(0, this.settings.samples);
    return { beam: this.settings.beam, max_dots: this.settings.maxDots, leaf_k: samples > 0 ? 16 : 32, samples, samples_top: samples > 0 ? samples * 2 : 0, sample_beam: 12, alts: 1 };
  }

  /** 손패 추첨 분포. null = 균등. */
  pieceWeights(): number[] | null {
    return this.settings.usePieceCounts ? store.pieceWeights() : null;
  }

  /** 지금 설정 요약 (화면·일괄 실행 라벨). */
  describe(): string {
    const s = store.state.settings;
    const total = s.counts.reduce((a, b) => a + b, 0);
    const dist = this.settings.usePieceCounts ? `기록 ${total}회 + α${s.alpha}` : '균등';
    return `가중치 ${s.weightsLabel} · 빔 ${this.settings.beam} · 샘플 ${this.settings.samples} · 조각 분포 ${dist}`;
  }

  private cfg(maxHands = 0): PlayConfig {
    return { max_hands: maxHands, replan_on_spawn: this.settings.replanOnSpawn, reroll_at_cap: this.settings.rerollAtCap, reroll_beam: 8, replan_each_move: false, reroll_margin: 0 };
  }

  newGame(seed: number): void {
    this.running = false;
    this.gen++;
    this.seed = seed;
    const r = simNew(seed, this.pieceWeights());
    this.state = r.state;
    this.rng = r.rng;
    this.stats = blankResult(seed);
    this.log = [];
    this.frame = null;
    this.stepCount = 0;
    this.elapsedMs = 0;
    this.lastStepMs = 0;
    this.pushLog('info', `새 게임 (시드 ${seed}). 손패: ${this.handNames(r.state.hand)}`);
    this.onChange();
  }

  handNames(hand: (number | null)[]): string {
    return hand.map((h) => (h === null ? '·' : app.piece(h).name)).join(' ');
  }

  private pushLog(kind: LogEntry['kind'], text: string): void {
    this.log.push({ step: this.stepCount, text, kind });
    if (this.log.length > 400) this.log.splice(0, this.log.length - 400);
  }

  /** 정책 한 수를 두고 애니메이션한다. 게임이 계속되면 true. */
  async step(): Promise<boolean> {
    if (!this.state || !this.rng || this.state.over || this.busy) return false;
    this.busy = true;
    const gen = this.gen;
    const t0 = performance.now();
    try {
      const before = this.state;
      const res = await this.getPool().simStep(before, this.rng, this.params(), store.state.settings.weights, this.pieceWeights(), this.cfg());
      if (gen !== this.gen) return false;
      this.lastStepMs = performance.now() - t0;
      this.elapsedMs += this.lastStepMs;
      this.stepCount++;
      this.stats = simRecord(this.stats, res.events);
      await this.animate(before, res.events, gen);
      if (gen !== this.gen) return false;
      this.state = res.state;
      this.rng = res.rng;
      this.frame = null;
      this.onChange();
      return !res.state.over;
    } catch (e) {
      if (!String(e).includes('cancelled')) this.pushLog('info', `오류: ${String(e)}`);
      this.running = false;
      this.onChange();
      return false;
    } finally {
      this.busy = false;
    }
  }

  async run(): Promise<void> {
    if (this.running) return;
    if (!this.state || this.state.over) this.newGame(this.seed);
    this.running = true;
    this.onChange();
    let n = 0;
    while (this.running && this.state) {
      if (this.state.over) {
        // 관전 중이면 일괄 실행이 끝날 때까지 다음 시드로 이어서 본다.
        if (this.spectating && this.batchRunning) {
          await sleep(DELAY[this.speed] * 2);
          if (!this.running) break;
          this.newGame(this.seed + 1);
          continue;
        }
        break;
      }
      const ok = await this.step();
      if (!ok && !(this.state && this.state.over)) break;
      // 최고 속도에서도 화면이 멈추지 않게 가끔 양보한다.
      if (this.speed === 'max' && ++n % 3 === 0) await sleep(0);
    }
    this.running = false;
    this.spectating = false;
    this.onChange();
  }

  pause(): void {
    this.running = false;
    this.spectating = false;
    this.onChange();
  }

  stopAll(): void {
    this.running = false;
    this.spectating = false;
    this.batchRunning = false;
    this.gen++;
    this.batchGen++;
    if (this.pool) this.pool.cancelAll();
    if (this.batchPool) this.batchPool.cancelAll();
    this.busy = false;
    this.onChange();
  }

  private pieceName(id: number): string {
    return app.piece(id).name;
  }

  private iconText(ic: Icon): string {
    return `${ic.kind === 'Reroll' ? '⇄' : '⊙'}(${ic.r + 1}행 ${ic.c + 1}열)`;
  }

  /** 이벤트를 순서대로 로그에 쓰고, 속도에 따라 중간 프레임을 보여준다. */
  private async animate(before: GameState, events: SimEvent[], gen: number): Promise<void> {
    const d = DELAY[this.speed];
    let cur = before;
    for (const ev of events) {
      if (gen !== this.gen) return;
      switch (ev.t) {
        case 'Plan':
          if (!ev.complete) this.pushLog('info', `손패를 다 놓을 수 없음 (최대 ${ev.moves}수). 능력 사용 검토.`);
          break;
        case 'Reroll': {
          this.pushLog('reroll', `${ev.forced ? '막혀서 ' : ''}바꿔 뽑기: 슬롯 ${ev.slot + 1} ${this.pieceName(ev.old)} → ${this.pieceName(ev.new)} (기대 ${Math.round(ev.ev - ev.base) >= 0 ? '+' : ''}${Math.round(ev.ev - ev.base).toLocaleString()})`);
          const hand = cur.hand.slice();
          hand[ev.slot] = ev.new;
          cur = { ...cur, hand, rerolls: cur.rerolls - 1 };
          if (d) {
            this.frame = { board: cur.board, icons: cur.icons, hand, hl: new Map(), rows: new Set() };
            this.onChange();
            await sleep(d);
          }
          break;
        }
        case 'Place': {
          const name = this.pieceName(ev.piece);
          const hl = new Map<string, string>();
          for (const [r, c] of ev.cells) hl.set(`${r},${c}`, 'p1');
          if (d) {
            this.frame = { board: cur.board, icons: cur.icons, hand: cur.hand, hl, rows: new Set(ev.cleared) };
            this.onChange();
            await sleep(d);
          }
          let next: GameState;
          try {
            next = applyMove(cur, { t: 'Place', slot: ev.slot, orient: ev.orient, r: ev.r, c: ev.c }, false).state;
          } catch {
            next = cur;
          }
          if (ev.spawned) next = { ...next, icons: [...next.icons, ev.spawned] };
          if (ev.new_hand) next = { ...next, hand: ev.new_hand.slice() };
          const parts = [`${name} → ${ev.r + 1}행 ${ev.c + 1}열 (+${ev.score_delta.toLocaleString()})`];
          if (ev.cleared.length) parts.push(`${ev.cleared.length}줄 제거 +${(300 * ev.cleared.length * ev.cleared.length).toLocaleString()}`);
          if (ev.granted.length) parts.push(`능력 획득 ${ev.granted.map((g) => (g.kind === 'Reroll' ? '⇄' : '⊙')).join('')}`);
          this.pushLog(ev.cleared.length ? 'clear' : 'place', parts.join(' · '));
          if (ev.dropped) this.pushLog('ability', `아이콘 3개 초과: 가장 오래된 ${this.iconText(ev.dropped)} 소멸`);
          if (ev.spawned) this.pushLog('ability', `7번째 배치: 아이콘 생성 ${this.iconText(ev.spawned)}`);
          if (ev.spawn_blocked) this.pushLog('ability', `7번째 배치지만 보유 ${ABILITY_CAP}개라 아이콘 생성 없음`);
          if (ev.new_hand) this.pushLog('info', `새 손패: ${this.handNames(ev.new_hand)}`);
          if (d && ev.cleared.length) {
            this.frame = { board: next.board, icons: next.icons, hand: next.hand, hl: new Map(), rows: new Set() };
            this.onChange();
            await sleep(d / 2);
          }
          cur = next;
          break;
        }
        case 'Dot': {
          const hl = new Map<string, string>([[`${ev.r},${ev.c}`, 'er']]);
          if (d) {
            this.frame = { board: cur.board, icons: cur.icons, hand: cur.hand, hl, rows: new Set(ev.cleared) };
            this.onChange();
            await sleep(d);
          }
          try {
            cur = applyMove(cur, { t: 'Dot', r: ev.r, c: ev.c }, false).state;
          } catch {
            /* 상태 어긋남은 다음 단계의 엔진 상태로 덮인다 */
          }
          const parts = [`${ev.forced ? '막혀서 ' : ''}점 찍기 → ${ev.r + 1}행 ${ev.c + 1}열`];
          if (ev.cleared.length) parts.push(`${ev.cleared.length}줄 제거 +${(300 * ev.cleared.length * ev.cleared.length).toLocaleString()}`);
          if (ev.granted.length) parts.push(`능력 획득 ${ev.granted.map((g) => (g.kind === 'Reroll' ? '⇄' : '⊙')).join('')}`);
          this.pushLog('dot', parts.join(' · '));
          break;
        }
        case 'GameOver':
          this.pushLog('over', `게임 종료: ${ev.reason}`);
          break;
      }
    }
  }

  /** 여러 판을 워커에 나눠 끝까지 돌린다. */
  async runBatch(n: number, seed: number, maxHands: number): Promise<void> {
    if (this.batchRunning) return;
    this.batchRunning = true;
    const gen = this.batchGen;
    this.batch = { done: 0, total: n, results: [], label: this.describe() };
    this.onChange();
    const pool = this.getBatchPool();
    // 왼쪽 화면이 놀고 있으면 같은 설정으로 한 판씩 관전을 시작한다 (일괄 실행과는 별개의 워커).
    if (!this.running && !this.busy) {
      this.spectating = true;
      this.newGame(seed);
      void this.run();
    }
    const chunk = 1;
    const tasks: Array<() => Promise<void>> = [];
    for (let i = 0; i < n; i += chunk) {
      const count = Math.min(chunk, n - i);
      const s = seed + i;
      tasks.push(async () => {
        const r = await pool.simGames(count, s, this.params(), store.state.settings.weights, this.pieceWeights(), this.cfg(maxHands));
        if (gen !== this.batchGen || !this.batch) return;
        this.batch.results.push(...r.results);
        this.batch.done += r.results.length;
        this.onChange();
      });
    }
    try {
      await Promise.all(tasks.map((t) => t().catch((e) => console.warn('일괄 실행 판 실패', e))));
    } finally {
      this.batchRunning = false;
      if (this.batch) this.batch.results.sort((a, b) => a.seed - b.seed);
      this.onChange();
    }
  }
}

export const simulator = new Simulator();
