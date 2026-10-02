//! 자기대전 시뮬레이터.
//!
//! `policy_step`이 한 번의 탐색 단위(한 수)를 두고 무슨 일이 있었는지 `Event`로 돌려준다.
//! 네이티브 자기대전(`play_game`)과 브라우저 시뮬레이터(wasm `sim_step_json`)가 같은 함수를 쓴다.

use crate::board::{H, W};
use crate::eval::{value_fast, Weights};
use crate::pieces::piece;
use crate::rng::Rng;
use crate::search::{reroll_analysis, search, Move, SearchParams};
use crate::state::{Icon, PieceWeights, State, ABILITY_CAP, SCORE_CAP};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PlayConfig {
    /// 손패 수 상한 (학습 시 시간 제한용). 0이면 무제한.
    pub max_hands: u32,
    /// 매 행동마다 다시 탐색할지 (느리지만 아이콘 생성에 반응). false면 손패 단위.
    pub replan_each_move: bool,
    /// 바꿔 뽑기 추천 기준: 기대값이 현재 계획보다 이만큼 높아야 사용.
    pub reroll_margin: f32,
    /// 손패를 다 놓을 수 있어도 보유가 가득 찼을 때 바꿔 뽑기를 검토할지.
    pub reroll_at_cap: bool,
    /// 바꿔 뽑기 분석에 쓰는 빔 (탐색 빔보다 작게).
    pub reroll_beam: usize,
    /// 아이콘이 생성된 직후(7의 배수 배치) 다시 계획할지. 새 아이콘 위치는 탐색이 모르는 정보다.
    pub replan_on_spawn: bool,
}

impl Default for PlayConfig {
    fn default() -> Self {
        PlayConfig { max_hands: 0, replan_each_move: false, reroll_margin: 0.0, reroll_at_cap: true, reroll_beam: 8, replan_on_spawn: true }
    }
}

/// 한 수 동안 일어난 일. 브라우저 시뮬레이터가 그대로 애니메이션·로그에 쓴다.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t")]
pub enum Event {
    /// 탐색 결과 요약.
    Plan { complete: bool, gain: u32, value: f32, moves: usize },
    /// 바꿔 뽑기 사용. `forced`면 아무것도 못 놓아 어쩔 수 없이 쓴 것.
    Reroll { slot: u8, old: u8, new: u8, ev: f32, base: f32, forced: bool },
    /// 조각 배치. `cleared`는 제거된 행, `granted`는 획득한 아이콘, `spawned`는 새로 생긴 아이콘,
    /// `dropped`는 소멸한 최고령 아이콘, `spawn_blocked`는 7번째 배치였지만 보유 7개라 생성이 없었음.
    Place {
        slot: u8,
        piece: u8,
        orient: u8,
        r: u8,
        c: u8,
        cells: Vec<(u8, u8)>,
        cleared: Vec<u8>,
        granted: Vec<Icon>,
        score_delta: u32,
        spawned: Option<Icon>,
        dropped: Option<Icon>,
        spawn_blocked: bool,
        new_hand: Option<[Option<u8>; 3]>,
    },
    /// 점 찍기. `forced`면 계획에 없었지만 막혀서 쓴 것.
    Dot { r: u8, c: u8, cleared: Vec<u8>, granted: Vec<Icon>, score_delta: u32, forced: bool },
    GameOver { reason: String },
}

fn rows_of(mask: u16) -> Vec<u8> {
    (0..H as u8).filter(|r| (mask >> r) & 1 == 1).collect()
}

fn over_reason(st: &State) -> String {
    if st.score >= SCORE_CAP {
        "500,000점 도달".into()
    } else {
        "놓을 곳이 없고 능력도 없음".into()
    }
}

/// 가장 나은 점 찍기 칸 (막혔을 때 비상용).
fn best_dot_cell(st: &State, w: &Weights) -> Option<(usize, usize)> {
    let mut best: Option<((usize, usize), f32)> = None;
    for r in 0..H {
        for c in 0..W {
            if st.board.get(r, c) {
                continue;
            }
            let mut s = st.clone();
            let out = s.dot(r, c);
            let v = out.score_delta as f32 + value_fast(&s, w);
            if best.map_or(true, |b| v > b.1) {
                best = Some(((r, c), v));
            }
        }
    }
    best.map(|b| b.0)
}

/// 정책 한 수: 탐색 → (필요하면) 바꿔 뽑기 → 계획 실행. 상태를 직접 바꾸고 일어난 일을 돌려준다.
pub fn policy_step(st: &mut State, rng: &mut Rng, pw: &PieceWeights, w: &Weights, sp: &SearchParams, cfg: &PlayConfig) -> Vec<Event> {
    let mut ev = Vec::new();
    if st.over {
        return ev;
    }
    let reroll_params = SearchParams { beam: cfg.reroll_beam.max(1), samples: 0, leaf_k: 8, alts: 1, ..sp.clone() };
    let plan = search(st, pw, w, sp);
    ev.push(Event::Plan { complete: plan.complete, gain: plan.gain, value: plan.value, moves: plan.moves.len() });

    // 바꿔 뽑기: 손패를 다 못 놓거나, 보유가 가득 차 생성이 막혀 있을 때 검토.
    let consider_reroll = st.rerolls > 0 && (!plan.complete || (cfg.reroll_at_cap && st.held() >= ABILITY_CAP));
    if consider_reroll {
        let rep = reroll_analysis(st, pw, w, &reroll_params);
        if let Some(best) = rep.best() {
            if best.ev > rep.base + cfg.reroll_margin {
                let slot = best.slot as usize;
                let old = st.hand[slot].unwrap_or(0);
                let new = st.reroll(slot, rng, pw);
                ev.push(Event::Reroll { slot: best.slot, old, new, ev: best.ev, base: rep.base, forced: false });
                return ev;
            }
        }
    }

    if plan.moves.is_empty() {
        // 아무것도 못 놓는다. 능력이 남아 있으면 게임은 끝나지 않는다.
        if st.rerolls > 0 {
            let rep = reroll_analysis(st, pw, w, &reroll_params);
            let slot = rep.best().map(|b| b.slot as usize).or_else(|| st.hand.iter().position(|h| h.is_some()));
            if let Some(slot) = slot {
                let old = st.hand[slot].unwrap_or(0);
                let (evv, base) = rep.best().map(|b| (b.ev, rep.base)).unwrap_or((0.0, 0.0));
                let new = st.reroll(slot, rng, pw);
                ev.push(Event::Reroll { slot: slot as u8, old, new, ev: evv, base, forced: true });
                return ev;
            }
        }
        if st.dots > 0 {
            if let Some((r, c)) = best_dot_cell(st, w) {
                let out = st.dot(r, c);
                ev.push(Event::Dot { r: r as u8, c: c as u8, cleared: rows_of(out.cleared_mask), granted: out.granted, score_delta: out.score_delta, forced: true });
                st.check_over();
                if st.over {
                    ev.push(Event::GameOver { reason: over_reason(st) });
                }
                return ev;
            }
        }
        st.over = true;
        ev.push(Event::GameOver { reason: over_reason(st) });
        return ev;
    }

    for mv in &plan.moves {
        let mut replan = cfg.replan_each_move;
        match *mv {
            Move::Place { slot, orient, r, c } => {
                let (slot, orient, r, c) = (slot as usize, orient as usize, r as usize, c as usize);
                let Some(pid) = st.hand.get(slot).copied().flatten() else { break };
                let o = &piece(pid).orients[orient];
                if r + o.h as usize > H || c + o.w as usize > W || !st.board.can_place(o, r, c) {
                    break; // 계획 시점과 상태가 어긋났다 (아이콘 변화 등). 다음 수에서 다시 계획한다.
                }
                let cells: Vec<(u8, u8)> = o.cells.iter().map(|&(a, b)| (a + r as u8, b + c as u8)).collect();
                let out = st.place_det(slot, orient, r, c);
                let spawned = if out.spawn_due && !st.over { st.spawn_icon(rng) } else { None };
                let spawn_blocked = out.spawn_due && spawned.is_none();
                let new_hand = if out.hand_empty && !st.over {
                    st.draw_hand(rng, pw);
                    Some(st.hand)
                } else {
                    None
                };
                st.check_over();
                if out.spawn_due && cfg.replan_on_spawn {
                    replan = true;
                }
                ev.push(Event::Place {
                    slot: slot as u8,
                    piece: pid,
                    orient: orient as u8,
                    r: r as u8,
                    c: c as u8,
                    cells,
                    cleared: rows_of(out.cleared_mask),
                    granted: out.granted,
                    score_delta: out.score_delta,
                    spawned,
                    dropped: out.dropped,
                    spawn_blocked,
                    new_hand,
                });
            }
            Move::Dot { r, c } => {
                if st.dots == 0 || st.board.get(r as usize, c as usize) {
                    break;
                }
                let out = st.dot(r as usize, c as usize);
                ev.push(Event::Dot { r, c, cleared: rows_of(out.cleared_mask), granted: out.granted, score_delta: out.score_delta, forced: false });
                st.check_over();
            }
            Move::Reroll { .. } => unreachable!("계획에는 바꿔 뽑기가 들어가지 않는다"),
        }
        if st.over {
            ev.push(Event::GameOver { reason: over_reason(st) });
            break;
        }
        if replan {
            break;
        }
    }
    if !plan.complete && !st.over {
        // 부분 계획을 실행했고 손패가 남아 있다. 다음 수에서 능력 사용 또는 종료 판정.
        st.check_over();
        if st.over {
            ev.push(Event::GameOver { reason: over_reason(st) });
        }
    }
    ev
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct GameResult {
    pub seed: u64,
    pub score: u32,
    pub lines: u32,
    pub hands: u32,
    pub placements: u32,
    /// 동시 제거 n줄 횟수 (index n, 0은 미사용).
    pub clears: [u32; 6],
    pub pickups: u32,
    pub dots_used: u32,
    pub rerolls_used: u32,
    pub capped: bool,
    pub truncated: bool,
}

/// 이벤트를 통계에 반영한다 (네이티브·wasm 공용).
pub fn record_events(res: &mut GameResult, events: &[Event]) {
    for e in events {
        match e {
            Event::Place { cleared, granted, new_hand, .. } => {
                res.placements += 1;
                res.pickups += granted.len() as u32;
                if !cleared.is_empty() {
                    res.clears[cleared.len().min(5)] += 1;
                }
                if new_hand.is_some() {
                    res.hands += 1;
                }
            }
            Event::Dot { cleared, granted, .. } => {
                res.dots_used += 1;
                res.pickups += granted.len() as u32;
                if !cleared.is_empty() {
                    res.clears[cleared.len().min(5)] += 1;
                }
            }
            Event::Reroll { .. } => res.rerolls_used += 1,
            _ => {}
        }
    }
}

pub fn play_game(seed: u64, pw: &PieceWeights, w: &Weights, sp: &SearchParams, cfg: &PlayConfig) -> GameResult {
    let mut rng = Rng::new(seed);
    let mut st = State::new_game(&mut rng, pw);
    let mut res = GameResult { seed, ..Default::default() };
    let mut guard = 0u32;
    while !st.over {
        if cfg.max_hands > 0 && res.hands >= cfg.max_hands {
            res.truncated = true;
            break;
        }
        guard += 1;
        if guard > 100_000 {
            break;
        }
        let events = policy_step(&mut st, &mut rng, pw, w, sp, cfg);
        record_events(&mut res, &events);
        if events.is_empty() {
            break;
        }
    }
    res.score = st.score;
    res.lines = st.lines;
    res.capped = st.score >= SCORE_CAP;
    res
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Summary {
    pub games: usize,
    pub mean: f64,
    pub median: f64,
    pub min: u32,
    pub max: u32,
    pub mean_lines: f64,
    pub mean_hands: f64,
    pub clears: [u64; 6],
    pub multi_ratio: f64,
    pub capped: usize,
}

pub fn summarize(results: &[GameResult]) -> Summary {
    if results.is_empty() {
        return Summary::default();
    }
    let mut scores: Vec<u32> = results.iter().map(|r| r.score).collect();
    scores.sort_unstable();
    let n = results.len();
    let mut s = Summary {
        games: n,
        mean: scores.iter().map(|&x| x as f64).sum::<f64>() / n as f64,
        median: if n % 2 == 0 { (scores[n / 2 - 1] as f64 + scores[n / 2] as f64) / 2.0 } else { scores[n / 2] as f64 },
        min: scores[0],
        max: scores[n - 1],
        mean_lines: results.iter().map(|r| r.lines as f64).sum::<f64>() / n as f64,
        mean_hands: results.iter().map(|r| r.hands as f64).sum::<f64>() / n as f64,
        clears: [0; 6],
        multi_ratio: 0.0,
        capped: results.iter().filter(|r| r.capped).count(),
    };
    for r in results {
        for k in 1..6 {
            s.clears[k] += r.clears[k] as u64;
        }
    }
    let total: u64 = s.clears[1..].iter().sum();
    let multi: u64 = s.clears[2..].iter().sum();
    s.multi_ratio = if total > 0 { multi as f64 / total as f64 } else { 0.0 };
    s
}
