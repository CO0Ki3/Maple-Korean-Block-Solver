// 앱 런타임 컨텍스트: 조각 테이블, 워커 풀, 계산·적용 흐름. UI 모듈은 여기만 의존한다.
import { applyMove, enginePieces, ensureWasm, normalizeState } from './engine/wasm';
import { WorkerPool } from './engine/pool';
import { canonKey, normCells } from './presses';
import { blankGame, emptySlot, store } from './store';
import type { Cell, GameState, Move, PieceInfo, PlaceOutcome, RerollReport } from './types';
import { ABILITY_CAP, H, W } from './types';

export interface PlanFrame {
  before: GameState;
  after: GameState;
  move: Move;
  cells: Cell[];
  /** 이 단계에서 제거되는 줄 (before 기준 행 번호). */
  rows: number[];
  outcome: PlaceOutcome | null;
}

type StatusKind = 'info' | 'ok' | 'warn' | 'err';

class App {
  pieces: PieceInfo[] = [];
  pool!: WorkerPool;
  frames: PlanFrame[] = [];
  rerollReport: RerollReport | null = null;
  busy = 0;
  private renderers: Array<() => void> = [];
  private byCanon = new Map<string, number>();
  private solveTimer: number | null = null;

  async init(): Promise<void> {
    await ensureWasm();
    this.pieces = enginePieces();
    for (const p of this.pieces) this.byCanon.set(canonKey(p.cells), p.id);
    this.pool = new WorkerPool(store.state.settings.workers);
  }

  onRender(fn: () => void): void {
    this.renderers.push(fn);
  }

  renderAll(): void {
    for (const fn of this.renderers) fn();
  }

  status(msg: string, kind: StatusKind = 'info'): void {
    const el = document.getElementById('status');
    if (!el) return;
    el.textContent = msg;
    el.className = `status ${kind}`;
  }

  piece(id: number): PieceInfo {
    return this.pieces[id];
  }

  /** 회전·반전을 무시하고 같은 모양의 조각 id. 없으면 null. */
  matchPiece(cells: Cell[]): number | null {
    const id = this.byCanon.get(canonKey(cells));
    return id === undefined ? null : id;
  }

  get game(): GameState {
    return store.state.game;
  }

  handCount(): number {
    return this.game.hand.filter((h) => h !== null).length;
  }

  /** 계획을 버리고 진행 중인 계산을 취소한다. 판·손패·능력이 바뀌면 호출. */
  invalidate(): void {
    store.plan = null;
    store.preview = 0;
    this.frames = [];
    this.rerollReport = null;
    if (this.pool) this.pool.cancelAll();
  }

  /** 상태를 바꾸는 모든 UI 동작의 공통 전처리. */
  mutate(fn: () => void, opts: { invalidate?: boolean; resolve?: boolean } = {}): void {
    store.snapshot();
    store.state.example = false;
    fn();
    // 편집으로 상황이 바뀌었으니 종료 플래그는 엔진 기준으로 다시 판정한다 (막힘 + 능력 0일 때만 true).
    store.state.game.over = false;
    try {
      store.state.game = normalizeState(store.state.game);
    } catch (e) {
      this.status(`상태 오류: ${String(e)}`, 'err');
    }
    if (opts.invalidate !== false) this.invalidate();
    store.save();
    this.renderAll();
    if (opts.resolve !== false) this.scheduleSolve();
  }

  scheduleSolve(): void {
    if (!store.state.settings.autoSolve) return;
    if (this.solveTimer !== null) window.clearTimeout(this.solveTimer);
    this.solveTimer = window.setTimeout(() => {
      this.solveTimer = null;
      void this.solve();
    }, 150);
  }

  async solve(): Promise<void> {
    if (this.handCount() === 0) {
      store.plan = null;
      this.frames = [];
      this.renderAll();
      return;
    }
    const gen = ++this.pool.gen;
    const s = store.state.settings;
    const samples = s.params.samples ?? 0;
    const t0 = performance.now();
    this.busy++;
    this.status('계산 중…');
    this.renderAll();
    try {
      // 1) 빔 탐색은 워커 하나. 샘플링을 켰으면 후보를 넉넉히 받아 2)에서 병렬 재평가한다.
      const base = { ...s.params, samples: 0, alts: samples > 0 ? Math.max(4, s.workers * 2) : s.params.alts ?? 3 };
      const plan = await this.pool.solve(this.game, base, s.weights, store.pieceWeights());
      if (gen !== this.pool.gen) return;
      if (samples > 0 && plan.complete) {
        const samplesTop = s.params.samples_top ?? 0;
        this.status(`후보 ${plan.alts.length + 1}개를 다음 손패 ${samples}개 샘플로 재평가 중…${samplesTop ? ` (상위 후보는 +${samplesTop})` : ''}`);
        const cands = [{ value: plan.value, gain: plan.gain, moves: plan.moves }, ...plan.alts];
        const la = { ...s.params, samples, samples_top: 0, alts: 1 };
        const scored = await Promise.all(
          cands.map(async (c, i) => {
            const leaf = this.applyMoves(this.game, c.moves);
            if (!leaf) return { c, v: -Infinity, i, leaf: null as GameState | null, la: -Infinity };
            // 공통 난수(같은 시드): 후보들을 같은 손패 표본으로 비교해야 순위가 잡음에 흔들리지 않는다.
            const v = await this.pool.lookahead(leaf, la, s.weights, store.pieceWeights(), 1000);
            return { c, v: c.gain + v, i, leaf, la: v };
          }),
        );
        if (gen !== this.pool.gen) return;
        scored.sort((a, b) => b.v - a.v);
        // 2단계: 상위 후보(최대 4개)에만 다른 표본을 더 뽑아 평균을 정밀하게 한다.
        if (samplesTop > 0) {
          const topN = Math.min(4, scored.length);
          const la2 = { ...s.params, samples: samplesTop, samples_top: 0, alts: 1 };
          await Promise.all(
            scored.slice(0, topN).map(async (x) => {
              if (!x.leaf) return;
              const v2 = await this.pool.lookahead(x.leaf, la2, s.weights, store.pieceWeights(), 2000);
              x.v = x.c.gain + (x.la * samples + v2 * samplesTop) / (samples + samplesTop);
            }),
          );
          if (gen !== this.pool.gen) return;
          scored.sort((a, b) => b.v - a.v);
        }
        const best = scored[0];
        plan.moves = best.c.moves;
        plan.gain = best.c.gain;
        plan.value = best.v;
        plan.alts = scored.slice(1, 4).map((x) => ({ value: x.v, gain: x.c.gain, moves: x.c.moves }));
      }
      store.plan = plan;
      store.preview = 0;
      this.simulatePlan();
      const ms = Math.round(performance.now() - t0);
      this.status(`계산 ${ms}ms · 노드 ${plan.nodes.toLocaleString()} · 리프 ${plan.leaves}${samples > 0 ? ` · 샘플 ${samples}×후보` : ''}`, 'ok');
    } catch (e) {
      if (String(e).includes('cancelled')) return;
      this.status(`계산 실패: ${String(e)}`, 'err');
    } finally {
      this.busy = Math.max(0, this.busy - 1);
      this.renderAll();
    }
  }

  /** 계획을 결정적으로 적용한 최종 상태. 적용 불가면 null. */
  applyMoves(start: GameState, moves: Move[]): GameState | null {
    let cur = start;
    for (const mv of moves) {
      try {
        cur = applyMove(cur, mv, false).state;
      } catch {
        return null;
      }
    }
    return cur;
  }

  async rerollAnalysis(): Promise<void> {
    if (this.handCount() === 0) return;
    const gen = this.pool.gen;
    const s = store.state.settings;
    const params = { ...s.params, beam: Math.max(8, Math.round((s.params.beam ?? 64) / 4)), samples: 0, leaf_k: 16, alts: 1 };
    this.busy++;
    this.status('바꿔 뽑기 분석 중… (슬롯마다 조각 19종 × 탐색)');
    this.renderAll();
    try {
      const rep = await this.pool.reroll(this.game, params, s.weights, store.pieceWeights(), null);
      if (gen !== this.pool.gen) return;
      this.rerollReport = rep;
      this.status('바꿔 뽑기 분석 완료', 'ok');
    } catch (e) {
      if (String(e).includes('cancelled')) return;
      this.status(`분석 실패: ${String(e)}`, 'err');
    } finally {
      this.busy = Math.max(0, this.busy - 1);
      this.renderAll();
    }
  }

  /** 계획을 결정적으로 시뮬레이션해 단계별 미리보기 프레임을 만든다. */
  simulatePlan(): void {
    this.frames = [];
    const plan = store.plan;
    if (!plan) return;
    let cur = this.game;
    for (const mv of plan.moves) {
      let cells: Cell[] = [];
      if (mv.t === 'Place') {
        const pid = cur.hand[mv.slot];
        if (pid === null || pid === undefined) break;
        const o = this.piece(pid).orients[mv.orient];
        cells = o.cells.map(([r, c]) => [r + mv.r, c + mv.c] as Cell);
      } else if (mv.t === 'Dot') {
        cells = [[mv.r, mv.c]];
      } else {
        break;
      }
      let res;
      try {
        res = applyMove(cur, mv, false);
      } catch (e) {
        console.warn('simulatePlan', e);
        break;
      }
      const tmp = cur.board.map((row) => row.split(''));
      for (const [r, c] of cells) tmp[r][c] = '#';
      const rows: number[] = [];
      for (let r = 0; r < H; r++) if (tmp[r].every((ch) => ch === '#')) rows.push(r);
      this.frames.push({ before: cur, after: res.state, move: mv, cells, rows, outcome: res.outcome });
      cur = res.state;
    }
  }

  /** 계획의 첫 단계를 실제 상태에 반영한다. 반환값은 안내 문구 (없으면 null). */
  applyOne(): string | null {
    const fr = this.frames[0];
    if (!fr || !store.plan) return null;
    store.snapshot();
    store.state.example = false;
    store.state.game = fr.after;
    if (fr.move.t === 'Place') store.state.slots[fr.move.slot] = emptySlot();
    store.plan.moves = store.plan.moves.slice(1);
    store.plan.alts = [];
    this.rerollReport = null;
    this.pool.gen++; // 진행 중이던 분석 결과는 이제 옛 상태의 것이다.
    const msgs: string[] = [];
    if (fr.outcome) {
      if (fr.outcome.lines) msgs.push(`${fr.outcome.lines}줄 제거 +${(fr.outcome.score_delta - fr.outcome.cells - fr.outcome.pickups * 50).toLocaleString()}점`);
      if (fr.outcome.pickups) msgs.push(`능력 ${fr.outcome.pickups}개 획득`);
      if (fr.outcome.spawn_due) {
        const heldAfter = fr.after.dots + fr.after.rerolls;
        msgs.push(heldAfter >= ABILITY_CAP ? '배치 7번째지만 보유 능력이 7개라 아이콘은 생기지 않습니다.' : '배치 7번째: 판 어딘가에 새 아이콘이 생겼을 거예요. 능력 아이콘 모드로 표시하세요.');
      }
      if (fr.outcome.icon_dropped) msgs.push('아이콘이 3개라 가장 오래된 아이콘은 사라졌습니다.');
    }
    if (this.handCount() === 0) {
      store.plan = null;
      this.frames = [];
      store.state.slots = [emptySlot(), emptySlot(), emptySlot()];
      store.state.target = 0;
      msgs.push('3개를 모두 놓았습니다. 새 손패를 넣거나 스크린샷을 붙여넣으세요.');
    } else if (!store.plan.moves.length) {
      store.plan = null;
      this.frames = [];
    } else {
      this.simulatePlan();
    }
    store.preview = 0;
    store.save();
    const msg = msgs.join(' · ') || '적용했습니다.';
    this.status(msg, 'ok');
    this.renderAll();
    // 남은 손패가 있으면 최신 상태로 다시 계산한다 (아이콘 변화 등 반영). 그동안 남은 계획은 그대로 보인다.
    if (this.handCount() > 0) this.scheduleSolve();
    return msg;
  }

  applyAll(): void {
    let guard = 8;
    const all: string[] = [];
    while (this.frames.length && guard-- > 0) {
      const m = this.applyOne();
      if (m === null) break;
      all.push(m);
    }
    if (all.length) this.status(all.map((m, i) => `${i + 1}) ${m}`).join('  '), 'ok');
  }

  /** 라이브러리에서 조각을 슬롯에 넣는다. */
  assignPiece(pid: number, slotIdx?: number, cur: Cell[] | null = null): void {
    this.mutate(() => {
      let i = slotIdx ?? store.state.target;
      const g = store.state.game;
      if (i < 0 || i > 2) i = 0;
      if (g.hand[i] !== null && slotIdx === undefined) {
        const free = g.hand.findIndex((h) => h === null);
        if (free >= 0) i = free;
      }
      g.hand[i] = pid;
      store.state.slots[i] = { cur: cur ? normCells(cur) : null, counted: false };
      this.countSlot(i);
      const nx = g.hand.findIndex((h) => h === null);
      store.state.target = nx >= 0 ? nx : i;
      g.over = false;
    });
  }

  private countSlot(i: number): void {
    const g = store.state.game;
    const m = store.state.slots[i];
    const pid = g.hand[i];
    if (pid !== null && !m.counted) {
      store.state.settings.counts[pid]++;
      m.counted = true;
    }
  }

  clearSlot(i: number): void {
    this.mutate(() => {
      store.state.game.hand[i] = null;
      store.state.slots[i] = emptySlot();
      store.state.target = i;
    });
  }

  /** 바꿔 뽑기를 실제 게임에서 썼을 때: 능력 1개 소모, 슬롯은 비워서 새 조각을 입력받는다. */
  useReroll(i: number): void {
    if (store.state.game.rerolls <= 0 || store.state.game.hand[i] === null) return;
    this.mutate(() => {
      const g = store.state.game;
      g.rerolls--;
      g.hand[i] = null;
      store.state.slots[i] = emptySlot();
      store.state.target = i;
    }, { resolve: false });
    this.status(`슬롯 ${i + 1}을 바꿔 뽑았습니다. 게임에 나온 새 조각을 라이브러리에서 눌러 넣어주세요.`, 'warn');
  }

  /** 점 찍기 사용 (판 클릭). */
  useDot(r: number, c: number): void {
    const g = store.state.game;
    if (g.dots <= 0 || g.board[r][c] === '#') return;
    this.mutate(() => {
      const res = applyMove(store.state.game, { t: 'Dot', r, c }, false);
      store.state.game = res.state;
      const o = res.outcome;
      if (o && (o.lines || o.pickups)) this.status(`점 찍기: ${o.lines ? `${o.lines}줄 제거 ` : ''}${o.pickups ? `능력 ${o.pickups}개 획득` : ''}`.trim(), 'ok');
    });
  }

  /** 칸 채움/비움. 아이콘은 칸이 채워져도 그 자리에 그대로 남고 순서도 바뀌지 않는다. */
  setCell(r: number, c: number, filled: boolean): void {
    const g = store.state.game;
    const row = g.board[r].split('');
    row[c] = filled ? '#' : '.';
    g.board[r] = row.join('');
  }

  /** 아이콘 모드 클릭: 없음 → 점 찍기 → 바꿔 뽑기 → 없음. 3개 초과면 가장 오래된 것부터 뺀다. */
  cycleIcon(r: number, c: number): void {
    this.mutate(() => {
      const g = store.state.game;
      const idx = g.icons.findIndex((ic) => ic.r === r && ic.c === c);
      if (idx < 0) {
        g.icons.push({ r, c, kind: 'Dot', seq: g.icon_seq++ });
        while (g.icons.length > 3) g.icons.shift();
      } else if (g.icons[idx].kind === 'Dot') {
        g.icons[idx].kind = 'Reroll';
      } else {
        g.icons.splice(idx, 1);
      }
    });
  }

  setGame(g: GameState): void {
    try {
      store.state.game = normalizeState(g);
    } catch (e) {
      this.status(`상태 오류: ${String(e)}`, 'err');
      store.state.game = g;
    }
  }

  newGame(): void {
    this.mutate(() => {
      store.state.game = blankGame();
      store.state.slots = [emptySlot(), emptySlot(), emptySlot()];
      store.state.target = 0;
    }, { resolve: false });
    this.status('새 판입니다. 손패 3개를 넣어주세요.');
  }

  heldText(): string {
    const g = this.game;
    return `${g.dots + g.rerolls}/${ABILITY_CAP}`;
  }

  nextSpawnIn(): number {
    const m = this.game.placements % 7;
    return m === 0 ? 7 : 7 - m;
  }

  boardFilled(): number {
    let n = 0;
    for (const row of this.game.board) for (const ch of row) if (ch === '#') n++;
    return n;
  }

  emptyCells(): number {
    return W * H - this.boardFilled();
  }
}

export const app = new App();
