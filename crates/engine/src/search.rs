//! 의사결정 탐색: 손패 내 빔 서치 + 점 찍기 행동 + (옵션) 다음 손패 샘플링 lookahead + 바꿔 뽑기 기대값.

use crate::board::{FULL, H, W};
use crate::eval::{value_fast, value_full, Weights};
use crate::pieces::{piece, NUM_PIECES};
use crate::rng::Rng;
use crate::state::{PieceWeights, State};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "t")]
pub enum Move {
    Place { slot: u8, orient: u8, r: u8, c: u8 },
    Dot { r: u8, c: u8 },
    Reroll { slot: u8 },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SearchParams {
    /// 레벨·버킷당 유지하는 노드 수.
    pub beam: usize,
    /// 한 탐색에서 점 찍기를 최대 몇 번까지 끼워 넣을지.
    pub max_dots: u8,
    /// 리프 중 정밀 평가(slow 특징, 샘플링) 대상 수.
    pub leaf_k: usize,
    /// 리프별 다음 손패 샘플 수 (0이면 끔).
    pub samples: usize,
    /// 2단계: 1단계 상위 1/4 리프에만 추가로 뽑는 샘플 수 (0이면 끔). 같은 연산으로 최상위 후보의 잡음을 줄인다.
    pub samples_top: usize,
    /// 샘플 탐색의 빔.
    pub sample_beam: usize,
    /// 대안 후보 수.
    pub alts: usize,
    /// 능력 없이 막힌 상태의 벌점.
    pub death_penalty: f32,
    /// 능력이 있어서 당장은 안 죽지만 손패를 다 못 놓는 상태의 벌점.
    pub stuck_penalty: f32,
    /// 다중 제거 편향: 탐색 내부 순위에만 300·k·n·(n−1)점을 더한다 (n = 동시 제거 줄 수). 실제 점수(gain)에는 넣지 않는다.
    /// 캡까지 손패 수를 줄이려면 줄당 점수가 높은 다중 제거를 더 자주 만들어야 한다.
    pub line_bonus: f32,
}

impl Default for SearchParams {
    fn default() -> Self {
        SearchParams {
            beam: 64,
            max_dots: 2,
            leaf_k: 64,
            samples: 0,
            samples_top: 0,
            sample_beam: 16,
            alts: 3,
            death_penalty: 50_000.0,
            stuck_penalty: 3_000.0,
            line_bonus: 0.0,
        }
    }
}

impl SearchParams {
    pub fn fast(beam: usize) -> Self {
        SearchParams { beam, max_dots: 1, leaf_k: 16, samples: 0, ..Default::default() }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Alt {
    pub value: f32,
    pub gain: u32,
    pub moves: Vec<Move>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Plan {
    /// 추천 행동 순서 (현재 손패 기준). 바꿔 뽑기는 포함하지 않는다 — 확률 행동이라 별도 분석.
    pub moves: Vec<Move>,
    /// 누적 실점수 + 리프 평가 (불완전하면 벌점 포함).
    pub value: f32,
    /// 이 계획으로 얻는 실제 점수.
    pub gain: u32,
    /// 손패를 전부 놓을 수 있는가.
    pub complete: bool,
    pub placed: u8,
    pub alts: Vec<Alt>,
    /// 평가한 리프 수 (진단용).
    pub leaves: u32,
    pub nodes: u32,
}

struct Node {
    st: State,
    gain: u32,
    /// 순위용 편향 누적 (다중 제거 보너스). gain과 달리 실제 점수가 아니다.
    bias: f32,
    f: f32,
    parent: u32,
    mv: Option<Move>,
    placed: u8,
    dots_used: u8,
}

type Key = ([u16; H], u8, u8, u8);

fn hand_mask(st: &State) -> u8 {
    let mut m = 0u8;
    for (i, h) in st.hand.iter().enumerate() {
        if h.is_some() {
            m |= 1 << i;
        }
    }
    m
}

fn key_of(st: &State) -> Key {
    (st.board.rows, hand_mask(st), st.dots, st.rerolls)
}

fn path_of(arena: &[Node], mut idx: u32) -> Vec<Move> {
    let mut v = Vec::new();
    while idx != u32::MAX {
        let n = &arena[idx as usize];
        if let Some(m) = n.mv {
            v.push(m);
        }
        idx = n.parent;
    }
    v.reverse();
    v
}

/// 점 찍기 후보 칸: 빈 칸이 2개 이하인 줄의 빈 칸 전부 + 사방이 막힌 빈 칸.
fn dot_candidates(st: &State) -> Vec<(usize, usize)> {
    let rows = &st.board.rows;
    let mut out = Vec::new();
    for r in 0..H {
        let x = rows[r];
        let e = !x & FULL;
        if e == 0 {
            continue;
        }
        let miss = e.count_ones();
        let up = if r > 0 { rows[r - 1] } else { FULL };
        let dn = if r + 1 < H { rows[r + 1] } else { FULL };
        let l = ((x << 1) | 1) & FULL;
        let rr = (x >> 1) | (1 << (W - 1));
        let iso = e & l & rr & up & dn;
        let cand = if miss <= 2 { e } else { iso };
        let mut m = cand;
        while m != 0 {
            let c = m.trailing_zeros() as usize;
            out.push((r, c));
            m &= m - 1;
        }
    }
    out
}

/// 남은 손패 중 어느 조각이든 더 이상 놓을 자리가 없으면 큰 벌점. 빔이 '놓을 수 있는 경로'를 버리지 않게 한다.
/// 조각이 하나 남았을 때는 정확한 판정이고, 둘 이상이면 필요조건 필터다.
fn unfit_penalty(st: &State) -> f32 {
    let mut tried = [false; NUM_PIECES];
    for h in st.hand.iter().flatten() {
        if tried[*h as usize] {
            continue;
        }
        tried[*h as usize] = true;
        if !st.board.fits(piece(*h)) {
            return 20_000.0;
        }
    }
    0.0
}

fn push_child(children: &mut Vec<Node>, dedup: &mut HashMap<Key, u32>, node: Node) {
    let k = key_of(&node.st);
    match dedup.get(&k) {
        Some(&i) => {
            let i = i as usize;
            if children[i].f < node.f {
                children[i] = node;
            }
        }
        None => {
            dedup.insert(k, children.len() as u32);
            children.push(node);
        }
    }
}

/// 다중 제거 편향: n줄 동시 제거에 300·k·n·(n−1). 1줄은 0.
#[inline]
fn line_bias(p: &SearchParams, lines: u32) -> f32 {
    if p.line_bonus == 0.0 || lines < 2 {
        0.0
    } else {
        p.line_bonus * 300.0 * (lines * (lines - 1)) as f32
    }
}

fn penalty(st: &State, p: &SearchParams) -> f32 {
    if st.held() > 0 {
        p.stuck_penalty
    } else {
        p.death_penalty
    }
}

pub fn search(st: &State, pw: &PieceWeights, w: &Weights, p: &SearchParams) -> Plan {
    let seed = 0x5EED_1234_ABCD_0001;
    let plan = search_inner(st, pw, w, p, seed);
    // 좁은 빔이 유일한 완전 배치를 놓쳤을 수 있다. 불완전하면 빔을 4배로 한 번 더 시도한다 (드문 경우라 비용이 작다).
    if !plan.complete && plan.placed > 0 && p.beam < 256 {
        let wide = SearchParams { beam: p.beam * 4, ..p.clone() };
        let retry = search_inner(st, pw, w, &wide, seed);
        if retry.complete {
            return retry;
        }
    }
    plan
}

fn search_inner(root: &State, pw: &PieceWeights, w: &Weights, p: &SearchParams, seed: u64) -> Plan {
    let n_pieces = root.hand_count();
    if n_pieces == 0 || root.over {
        return Plan {
            moves: vec![],
            value: value_full(root, pw, w),
            gain: 0,
            complete: true,
            placed: 0,
            alts: vec![],
            leaves: 0,
            nodes: 0,
        };
    }
    let beam_w = p.beam.max(1);
    let mut arena: Vec<Node> = vec![Node {
        st: root.clone(),
        gain: 0,
        bias: 0.0,
        f: 0.0,
        parent: u32::MAX,
        mv: None,
        placed: 0,
        dots_used: 0,
    }];
    let mut beam: Vec<u32> = vec![0];
    let mut finished: Vec<u32> = Vec::new();
    let mut best_partial: u32 = 0;
    let mut nodes = 0u32;
    let max_levels = n_pieces + p.max_dots as usize;

    for _level in 0..max_levels {
        if beam.is_empty() {
            break;
        }
        let mut children: Vec<Node> = Vec::new();
        let mut dedup: HashMap<Key, u32> = HashMap::new();
        for &ni in &beam {
            let (st, gain, bias0, placed, dots_used) = {
                let n = &arena[ni as usize];
                (n.st.clone(), n.gain, n.bias, n.placed, n.dots_used)
            };
            let mut tried = [false; NUM_PIECES];
            for slot in 0..3 {
                let Some(pid) = st.hand[slot] else { continue };
                if tried[pid as usize] {
                    continue;
                }
                tried[pid as usize] = true;
                let pc = piece(pid);
                for (oi, o) in pc.orients.iter().enumerate() {
                    let (h, wd) = (o.h as usize, o.w as usize);
                    for r in 0..=(H - h) {
                        for c in 0..=(W - wd) {
                            if !st.board.can_place(o, r, c) {
                                continue;
                            }
                            let mut ch = st.clone();
                            let out = ch.place_det(slot, oi, r, c);
                            let g = gain + out.score_delta;
                            let bias = bias0 + line_bias(p, out.lines);
                            let f = g as f32 + bias + value_fast(&ch, w) - unfit_penalty(&ch);
                            nodes += 1;
                            push_child(
                                &mut children,
                                &mut dedup,
                                Node {
                                    st: ch,
                                    gain: g,
                                    bias,
                                    f,
                                    parent: ni,
                                    mv: Some(Move::Place { slot: slot as u8, orient: oi as u8, r: r as u8, c: c as u8 }),
                                    placed: placed + 1,
                                    dots_used,
                                },
                            );
                        }
                    }
                }
            }
            if dots_used < p.max_dots && st.dots > 0 {
                for (r, c) in dot_candidates(&st) {
                    let mut ch = st.clone();
                    let out = ch.dot(r, c);
                    let g = gain + out.score_delta;
                    let bias = bias0 + line_bias(p, out.lines);
                    let f = g as f32 + bias + value_fast(&ch, w) - unfit_penalty(&ch);
                    nodes += 1;
                    push_child(
                        &mut children,
                        &mut dedup,
                        Node {
                            st: ch,
                            gain: g,
                            bias,
                            f,
                            parent: ni,
                            mv: Some(Move::Dot { r: r as u8, c: c as u8 }),
                            placed,
                            dots_used: dots_used + 1,
                        },
                    );
                }
            }
        }
        if children.is_empty() {
            break;
        }
        // 버킷(놓은 수, 점 찍기 수)별로 상위 beam개.
        let mut order: Vec<usize> = (0..children.len()).collect();
        order.sort_unstable_by(|&a, &b| {
            let ka = (children[a].placed, children[a].dots_used);
            let kb = (children[b].placed, children[b].dots_used);
            ka.cmp(&kb).then_with(|| children[b].f.partial_cmp(&children[a].f).unwrap_or(std::cmp::Ordering::Equal))
        });
        let mut survivors: Vec<usize> = Vec::new();
        let mut cur_key = (255u8, 255u8);
        let mut cnt = 0usize;
        for &i in &order {
            let k = (children[i].placed, children[i].dots_used);
            if k != cur_key {
                cur_key = k;
                cnt = 0;
            }
            if cnt < beam_w {
                survivors.push(i);
                cnt += 1;
            }
        }
        beam.clear();
        // survivors를 arena로 옮긴다 (인덱스 내림차순으로 swap_remove 하면 안전).
        let mut taken: Vec<(usize, Node)> = Vec::with_capacity(survivors.len());
        survivors.sort_unstable();
        let mut children_opt: Vec<Option<Node>> = children.into_iter().map(Some).collect();
        for &i in &survivors {
            taken.push((i, children_opt[i].take().unwrap()));
        }
        for (_, node) in taken {
            let idx = arena.len() as u32;
            let done = node.st.hand_empty();
            // 루트(f=0)는 표식일 뿐이다: 실제 자식(점 찍기만 한 경로 포함)은 항상 루트보다 낫다.
            let better_partial = best_partial == 0 || {
                let bp = &arena[best_partial as usize];
                (node.placed, node.f) > (bp.placed, bp.f)
            };
            arena.push(node);
            if better_partial {
                best_partial = idx;
            }
            if done {
                finished.push(idx);
            } else {
                beam.push(idx);
            }
        }
    }

    if finished.is_empty() {
        let bp = &arena[best_partial as usize];
        let v = bp.gain as f32 + bp.bias + value_full(&bp.st, pw, w) - penalty(&bp.st, p);
        return Plan {
            moves: path_of(&arena, best_partial),
            value: v,
            gain: bp.gain,
            complete: false,
            placed: bp.placed,
            alts: vec![],
            leaves: 0,
            nodes,
        };
    }

    // 리프 정밀 평가: 같은 (판, 능력)이면 gain이 큰 것만.
    finished.sort_unstable_by(|&a, &b| arena[b as usize].f.partial_cmp(&arena[a as usize].f).unwrap_or(std::cmp::Ordering::Equal));
    let mut seen: HashMap<Key, ()> = HashMap::new();
    let mut top: Vec<(u32, f32)> = Vec::new();
    for &i in &finished {
        let k = key_of(&arena[i as usize].st);
        if seen.contains_key(&k) {
            continue;
        }
        seen.insert(k, ());
        let n = &arena[i as usize];
        let v = n.gain as f32 + n.bias + value_full(&n.st, pw, w);
        top.push((i, v));
        if top.len() >= p.leaf_k.max(1) {
            break;
        }
    }
    top.sort_unstable_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    if p.samples > 0 {
        let k = top.len().min(p.leaf_k.max(1));
        for item in top.iter_mut().take(k) {
            let n = &arena[item.0 as usize];
            // 공통 난수: 모든 리프를 같은 손패 표본으로 평가해야 순위가 표본 잡음에 흔들리지 않는다.
            item.1 = n.gain as f32 + n.bias + lookahead_value(&n.st, pw, w, p, seed);
        }
        top.sort_unstable_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        // 2단계: 상위 1/4 리프에만 다른 표본을 더 뽑아 평균을 정밀하게 한다 (successive halving).
        if p.samples_top > 0 && k > 0 {
            let k2 = (k / 4).max(1).min(k);
            let p2 = SearchParams { samples: p.samples_top, ..p.clone() };
            let seed2 = seed ^ 0x5151_5151_5151_5151;
            let (n1, n2) = (p.samples as f32, p.samples_top as f32);
            for item in top.iter_mut().take(k2) {
                let n = &arena[item.0 as usize];
                let v1 = item.1 - n.gain as f32 - n.bias;
                let v2 = lookahead_value(&n.st, pw, w, &p2, seed2);
                item.1 = n.gain as f32 + n.bias + (v1 * n1 + v2 * n2) / (n1 + n2);
            }
            top.sort_unstable_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        }
    }

    let mut alts: Vec<Alt> = Vec::new();
    let mut first_seen: Vec<Move> = Vec::new();
    for &(i, v) in &top {
        let moves = path_of(&arena, i);
        let first = moves[0];
        if first_seen.contains(&first) {
            continue;
        }
        first_seen.push(first);
        alts.push(Alt { value: v, gain: arena[i as usize].gain, moves });
        if alts.len() >= p.alts.max(1) {
            break;
        }
    }
    let best = alts.remove(0);
    Plan {
        moves: best.moves,
        value: best.value,
        gain: best.gain,
        complete: true,
        placed: n_pieces as u8,
        alts,
        leaves: top.len() as u32,
        nodes,
    }
}

/// 리프 상태(손패를 다 놓은 뒤)에서 다음 손패를 `p.samples`번 샘플링해 얕은 탐색을 돌린 평균 가치.
/// 바깥에서 후보 계획별로 병렬 호출할 수 있게 공개한다.
pub fn lookahead(leaf: &State, pw: &PieceWeights, w: &Weights, p: &SearchParams, seed: u64) -> f32 {
    if p.samples == 0 {
        return value_full(leaf, pw, w);
    }
    lookahead_value(leaf, pw, w, p, seed)
}

fn lookahead_value(leaf: &State, pw: &PieceWeights, w: &Weights, p: &SearchParams, seed: u64) -> f32 {
    let sub = SearchParams {
        beam: p.sample_beam.max(1),
        max_dots: p.max_dots.min(1),
        leaf_k: 8,
        samples: 0,
        samples_top: 0,
        sample_beam: 0,
        alts: 1,
        death_penalty: p.death_penalty,
        stuck_penalty: p.stuck_penalty,
        line_bonus: p.line_bonus,
    };
    let mut acc = 0f32;
    for m in 0..p.samples {
        let mut s = leaf.clone();
        let mut rng = Rng::new(seed.wrapping_add((m as u64 + 1).wrapping_mul(0xD1B5_4A32_D192_ED03)));
        s.draw_hand(&mut rng, pw);
        let pl = search_inner(&s, pw, w, &sub, seed ^ 0xABCD);
        acc += pl.value;
    }
    acc / p.samples as f32
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PieceOutcome {
    pub piece: u8,
    pub prob: f32,
    pub value: f32,
    pub complete: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RerollOption {
    pub slot: u8,
    /// 가중치로 평균낸 기대 가치 (바꿔 뽑기 1개 소모 반영).
    pub ev: f32,
    pub per_piece: Vec<PieceOutcome>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RerollReport {
    pub base: f32,
    pub base_complete: bool,
    pub options: Vec<RerollOption>,
}

impl RerollReport {
    pub fn best(&self) -> Option<&RerollOption> {
        self.options.iter().max_by(|a, b| a.ev.partial_cmp(&b.ev).unwrap_or(std::cmp::Ordering::Equal))
    }
}

/// 한 슬롯을 바꿔 뽑았을 때의 기대 가치 (바꿔 뽑기 1개 소모 반영). 빈 슬롯이면 None.
pub fn reroll_option(st: &State, pw: &PieceWeights, w: &Weights, p: &SearchParams, slot: usize) -> Option<RerollOption> {
    st.hand.get(slot).copied().flatten()?;
    let probs = pw.probs();
    let mut ev = 0f32;
    let mut per_piece = Vec::new();
    for pid in 0..NUM_PIECES {
        let pr = probs[pid] as f32;
        if pr <= 0.0 {
            continue;
        }
        let mut s = st.clone();
        s.rerolls = s.rerolls.saturating_sub(1);
        s.hand[slot] = Some(pid as u8);
        let pl = search(&s, pw, w, p);
        ev += pr * pl.value;
        per_piece.push(PieceOutcome { piece: pid as u8, prob: pr, value: pl.value, complete: pl.complete });
    }
    Some(RerollOption { slot: slot as u8, ev, per_piece })
}

/// 각 슬롯을 바꿔 뽑았을 때의 기대 가치. `base`는 지금 손패 그대로의 가치.
pub fn reroll_analysis(st: &State, pw: &PieceWeights, w: &Weights, p: &SearchParams) -> RerollReport {
    let base = search(st, pw, w, p);
    let options: Vec<RerollOption> = (0..3).filter_map(|slot| reroll_option(st, pw, w, p, slot)).collect();
    let mut base_v = base.value;
    // 아무것도 못 놓고 점 찍기도 없으면 바꿔 뽑기가 유일한 행동이다. '그대로 두는' 가치는 최선의 바꿔 뽑기 기대값과 같다.
    if !base.complete && base.placed == 0 && st.dots == 0 && st.rerolls > 0 {
        if let Some(b) = options.iter().max_by(|a, b| a.ev.partial_cmp(&b.ev).unwrap_or(std::cmp::Ordering::Equal)) {
            base_v = b.ev;
        }
    }
    RerollReport { base: base_v, base_complete: base.complete, options }
}
