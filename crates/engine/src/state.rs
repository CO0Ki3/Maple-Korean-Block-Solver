//! 게임 상태와 규칙(상태 전이). 규칙 원문은 docs/DESIGN.md 2장.

use crate::board::{Board, FULL, H, W};
use crate::pieces::{piece, NUM_PIECES};
use crate::rng::Rng;
use serde::{Deserialize, Serialize};

/// 보유 능력 합산 한도.
pub const ABILITY_CAP: u8 = 7;
/// 판 위 능력 아이콘 최대 개수. 초과 시 가장 먼저 생성된 것부터 사라진다.
pub const ICON_CAP: usize = 3;
/// 조각 배치 n번마다 아이콘 생성 (점 찍기는 세지 않음).
pub const SPAWN_EVERY: u32 = 7;
/// 한 판 최대 점수.
pub const SCORE_CAP: u32 = 500_000;
/// 능력 획득 점수.
pub const PICKUP_SCORE: u32 = 50;
/// 아이콘 종류 확률: 점 찍기 40%, 바꿔 뽑기 60%.
pub const P_DOT: f64 = 0.40;

/// 동시 제거 n줄 점수: 300·n² (1줄 300, 2줄 1,200, 3줄 2,700, 4줄 4,800, 5줄 7,500).
#[inline]
pub const fn line_score(n: u32) -> u32 {
    300 * n * n
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, Serialize, Deserialize)]
pub enum AbilityKind {
    #[default]
    Dot,
    Reroll,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, Serialize, Deserialize)]
pub struct Icon {
    pub r: u8,
    pub c: u8,
    pub kind: AbilityKind,
    /// 생성 순서 (작을수록 오래됨).
    pub seq: u32,
}

/// 판 위 아이콘 목록. 오래된 것이 앞.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct Icons {
    items: [Icon; ICON_CAP],
    len: u8,
}

impl Icons {
    pub fn len(&self) -> usize {
        self.len as usize
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub fn iter(&self) -> impl Iterator<Item = &Icon> {
        self.items[..self.len as usize].iter()
    }
    pub fn get(&self, i: usize) -> Icon {
        self.items[i]
    }
    pub fn push(&mut self, ic: Icon) {
        debug_assert!(self.len() < ICON_CAP);
        self.items[self.len as usize] = ic;
        self.len += 1;
    }
    pub fn remove(&mut self, i: usize) -> Icon {
        let n = self.len as usize;
        debug_assert!(i < n);
        let ic = self.items[i];
        for k in i..n - 1 {
            self.items[k] = self.items[k + 1];
        }
        self.items[n - 1] = Icon::default();
        self.len -= 1;
        ic
    }
    pub fn at(&self, r: u8, c: u8) -> Option<usize> {
        self.iter().position(|ic| ic.r == r && ic.c == c)
    }
    pub fn to_vec(&self) -> Vec<Icon> {
        self.iter().copied().collect()
    }
}

/// 조각 등장 가중치. 단계(누적 줄 수)별 변화는 모델링하지 않는다 (사용자 결정).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PieceWeights {
    pub w: [f64; NUM_PIECES],
}

impl Default for PieceWeights {
    fn default() -> Self {
        PieceWeights::uniform()
    }
}

impl PieceWeights {
    pub fn uniform() -> Self {
        PieceWeights { w: [1.0; NUM_PIECES] }
    }

    /// 관측 횟수 + 라플라스 보정.
    pub fn from_counts(counts: &[u32; NUM_PIECES], alpha: f64) -> Self {
        let mut w = [0.0; NUM_PIECES];
        for i in 0..NUM_PIECES {
            w[i] = counts[i] as f64 + alpha;
        }
        PieceWeights { w }
    }

    pub fn total(&self) -> f64 {
        self.w.iter().sum()
    }

    pub fn prob(&self, id: u8) -> f64 {
        let t = self.total();
        if t <= 0.0 {
            0.0
        } else {
            self.w[id as usize] / t
        }
    }

    pub fn probs(&self) -> [f64; NUM_PIECES] {
        let t = self.total();
        let mut p = [0.0; NUM_PIECES];
        if t > 0.0 {
            for i in 0..NUM_PIECES {
                p[i] = self.w[i] / t;
            }
        }
        p
    }

    pub fn sample(&self, rng: &mut Rng) -> u8 {
        let t = self.total();
        let mut x = rng.next_f64() * t;
        for i in 0..NUM_PIECES {
            x -= self.w[i];
            if x < 0.0 {
                return i as u8;
            }
        }
        // 부동소수 오차: 가중치가 0이 아닌 마지막 조각.
        (0..NUM_PIECES).rev().find(|&i| self.w[i] > 0.0).unwrap_or(0) as u8
    }
}

/// 배치/점 찍기 결과 요약.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaceOutcome {
    pub cells: u32,
    pub lines: u32,
    pub pickups: u32,
    /// 제거된 행의 비트마스크 (행 r → 비트 r).
    #[serde(default)]
    pub cleared_mask: u16,
    /// 이번에 획득한 아이콘들.
    #[serde(default)]
    pub granted: Vec<Icon>,
    /// 아이콘 3개 상태에서 생성이 확정되어 사라진 최고령 아이콘.
    #[serde(default)]
    pub dropped: Option<Icon>,
    /// 이번 배치로 배치 횟수가 7의 배수가 되어 아이콘 생성 판정이 있는가.
    pub spawn_due: bool,
    /// 이번 행동 후 손패가 비었는가 (새 손패를 뽑아야 함).
    pub hand_empty: bool,
    /// 아이콘 생성이 확정되어(보유 < 7, 빈 칸 있음) 판 위 아이콘이 3개였기에 가장 오래된 아이콘이 사라졌는가.
    pub icon_dropped: bool,
    pub score_delta: u32,
}

/// 게임 상태. `place_det`/`dot`는 결정적 전이이고, 아이콘 생성과 손패 추첨만 난수를 쓴다.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(into = "StateDto", try_from = "StateDto")]
pub struct State {
    pub board: Board,
    /// 손패 3슬롯. None = 이미 배치됨(비어 있음).
    pub hand: [Option<u8>; 3],
    pub dots: u8,
    pub rerolls: u8,
    pub icons: Icons,
    /// 누적 조각 배치 횟수 (점 찍기 제외).
    pub placements: u32,
    pub lines: u32,
    pub score: u32,
    pub icon_seq: u32,
    pub over: bool,
}

#[derive(Serialize, Deserialize)]
struct StateDto {
    board: Board,
    hand: [Option<u8>; 3],
    dots: u8,
    rerolls: u8,
    icons: Vec<Icon>,
    placements: u32,
    lines: u32,
    score: u32,
    #[serde(default)]
    icon_seq: u32,
    #[serde(default)]
    over: bool,
}

impl From<State> for StateDto {
    fn from(s: State) -> Self {
        StateDto {
            board: s.board,
            hand: s.hand,
            dots: s.dots,
            rerolls: s.rerolls,
            icons: s.icons.to_vec(),
            placements: s.placements,
            lines: s.lines,
            score: s.score,
            icon_seq: s.icon_seq,
            over: s.over,
        }
    }
}

impl TryFrom<StateDto> for State {
    type Error = String;
    fn try_from(d: StateDto) -> Result<Self, String> {
        if d.icons.len() > ICON_CAP {
            return Err(format!("아이콘은 최대 {}개입니다", ICON_CAP));
        }
        let mut icons = Icons::default();
        let mut max_seq = 0;
        for ic in &d.icons {
            if ic.r as usize >= H || ic.c as usize >= W {
                return Err(format!("아이콘 위치가 판 밖입니다: ({}, {})", ic.r, ic.c));
            }
            icons.push(*ic);
            max_seq = max_seq.max(ic.seq + 1);
        }
        for h in d.hand.iter().flatten() {
            if *h as usize >= NUM_PIECES {
                return Err(format!("조각 id가 범위를 벗어났습니다: {}", h));
            }
        }
        if d.dots + d.rerolls > ABILITY_CAP {
            return Err(format!("보유 능력은 합쳐서 {}개까지입니다", ABILITY_CAP));
        }
        // `over`는 저장된 값을 믿지 않는다. 판을 고친 뒤에도 옛 플래그가 남아 빈 계획이 나오는 일을 막는다.
        let mut st = State {
            board: d.board,
            hand: d.hand,
            dots: d.dots,
            rerolls: d.rerolls,
            icons,
            placements: d.placements,
            lines: d.lines,
            score: d.score,
            icon_seq: d.icon_seq.max(max_seq),
            over: false,
        };
        st.over = st.score >= SCORE_CAP || st.is_stuck();
        Ok(st)
    }
}

impl Default for State {
    fn default() -> Self {
        State::blank()
    }
}

impl State {
    /// 빈 판, 빈 손패.
    pub fn blank() -> State {
        State {
            board: Board::empty(),
            hand: [None; 3],
            dots: 0,
            rerolls: 0,
            icons: Icons::default(),
            placements: 0,
            lines: 0,
            score: 0,
            icon_seq: 0,
            over: false,
        }
    }

    /// 새 게임: 빈 판에 손패 3개.
    pub fn new_game(rng: &mut Rng, pw: &PieceWeights) -> State {
        let mut s = State::blank();
        s.draw_hand(rng, pw);
        s
    }

    #[inline]
    pub fn held(&self) -> u8 {
        self.dots + self.rerolls
    }

    pub fn hand_count(&self) -> usize {
        self.hand.iter().filter(|h| h.is_some()).count()
    }

    pub fn hand_empty(&self) -> bool {
        self.hand.iter().all(|h| h.is_none())
    }

    /// 가득 찬 줄을 제거하고 점수·능력을 정산한다. (줄 수, 획득한 아이콘, 제거 행 마스크)
    fn resolve_clears(&mut self) -> (u32, Vec<Icon>, u16) {
        let mask = self.board.full_mask();
        if mask == 0 {
            return (0, Vec::new(), 0);
        }
        let n = mask.count_ones();
        self.board.clear_mask(mask);
        self.lines += n;
        self.score += line_score(n);
        // 아이콘: 오래된 것부터. 보유 한도에 닿으면 획득하지 못하고 판에 남는다.
        let mut granted = Vec::new();
        let mut i = 0;
        while i < self.icons.len() {
            let ic = self.icons.get(i);
            if (mask >> ic.r) & 1 == 1 && self.held() < ABILITY_CAP {
                match ic.kind {
                    AbilityKind::Dot => self.dots += 1,
                    AbilityKind::Reroll => self.rerolls += 1,
                }
                self.score += PICKUP_SCORE;
                granted.push(self.icons.remove(i));
            } else {
                i += 1;
            }
        }
        (n, granted, mask)
    }

    /// 아이콘을 둘 수 있는 칸(아이콘 없는 빈 칸) 수.
    pub fn spawnable_cells(&self) -> u32 {
        let mut total = 0u32;
        for r in 0..H {
            let mut e = !self.board.rows[r] & FULL;
            for ic in self.icons.iter() {
                if ic.r as usize == r {
                    e &= !(1 << ic.c);
                }
            }
            total += e.count_ones();
        }
        total
    }

    /// 이번 배치로 아이콘 생성이 확정되는가 (7의 배수, 보유 < 7, 둘 칸 있음).
    pub fn spawn_will_happen(&self) -> bool {
        self.placements % SPAWN_EVERY == 0 && self.held() < ABILITY_CAP && self.spawnable_cells() > 0
    }

    #[inline]
    fn cap_check(&mut self) {
        if self.score >= SCORE_CAP {
            self.score = SCORE_CAP;
            self.over = true;
        }
    }

    /// 결정적 배치: 칸 채움 → 점수 → 줄 제거·능력 획득 → 손패 소모 → 배치 카운터.
    /// 아이콘 생성과 새 손패 추첨은 하지 않는다 (탐색에서 확률 노드를 분리하기 위함).
    pub fn place_det(&mut self, slot: usize, orient: usize, r: usize, c: usize) -> PlaceOutcome {
        let pid = self.hand[slot].expect("빈 슬롯을 배치할 수 없습니다");
        let o = &piece(pid).orients[orient];
        assert!(r + o.h as usize <= H && c + o.w as usize <= W, "판 밖 배치");
        assert!(self.board.can_place(o, r, c), "겹치는 배치");
        self.board.place(o, r, c);
        let cells = o.n as u32;
        self.score += cells;
        let (lines, granted, cleared_mask) = self.resolve_clears();
        let pickups = granted.len() as u32;
        self.hand[slot] = None;
        self.placements += 1;
        self.cap_check();
        // 생성이 확정되고 판에 아이콘이 이미 3개면 가장 오래된 것이 사라진다. 이 부분은 결정적이므로
        // 탐색이 같은 전이를 보게 여기서 처리한다. 새 아이콘의 위치·종류만 `spawn_icon`(난수)이 정한다.
        let spawn_due = self.placements % SPAWN_EVERY == 0;
        let mut dropped = None;
        if spawn_due && !self.over && self.icons.len() >= ICON_CAP && self.spawn_will_happen() {
            dropped = Some(self.icons.remove(0));
        }
        PlaceOutcome {
            cells,
            lines,
            pickups,
            cleared_mask,
            granted,
            icon_dropped: dropped.is_some(),
            dropped,
            spawn_due,
            hand_empty: self.hand_empty(),
            score_delta: cells + line_score(lines) + pickups * PICKUP_SCORE,
        }
    }

    /// 아이콘 생성 판정. 보유 7개면 생성하지 않는다. 위치는 아이콘 없는 빈 칸 중 균등 (가정 A1).
    /// 4번째가 생기면 가장 오래된 아이콘이 사라진다.
    pub fn spawn_icon(&mut self, rng: &mut Rng) -> Option<Icon> {
        if self.held() >= ABILITY_CAP {
            return None;
        }
        let total = self.spawnable_cells();
        if total == 0 {
            return None;
        }
        let kind = if rng.next_f64() < P_DOT { AbilityKind::Dot } else { AbilityKind::Reroll };
        let mut k = rng.below(total);
        for r in 0..H {
            let mut e = !self.board.rows[r] & FULL;
            for ic in self.icons.iter() {
                if ic.r as usize == r {
                    e &= !(1 << ic.c);
                }
            }
            let n = e.count_ones();
            if k < n {
                let mut m = e;
                for _ in 0..k {
                    m &= m - 1;
                }
                let c = m.trailing_zeros() as u8;
                if self.icons.len() >= ICON_CAP {
                    self.icons.remove(0);
                }
                let ic = Icon { r: r as u8, c, kind, seq: self.icon_seq };
                self.icon_seq += 1;
                self.icons.push(ic);
                return Some(ic);
            }
            k -= n;
        }
        None
    }

    pub fn draw_hand(&mut self, rng: &mut Rng, pw: &PieceWeights) {
        for s in 0..3 {
            self.hand[s] = Some(pw.sample(rng));
        }
    }

    /// 실제 게임의 배치 한 번: 결정적 전이 + 아이콘 생성 + (손패를 다 썼으면) 새 손패 + 종료 판정.
    pub fn place(&mut self, slot: usize, orient: usize, r: usize, c: usize, rng: &mut Rng, pw: &PieceWeights) -> PlaceOutcome {
        let out = self.place_det(slot, orient, r, c);
        if out.spawn_due && !self.over {
            self.spawn_icon(rng);
        }
        if out.hand_empty && !self.over {
            self.draw_hand(rng, pw);
        }
        self.check_over();
        out
    }

    /// 점 찍기: 빈 칸 1칸을 채운다. 배치 점수 1점, 줄 제거·능력 획득은 정상 처리, 배치 카운터는 증가하지 않는다.
    pub fn dot(&mut self, r: usize, c: usize) -> PlaceOutcome {
        assert!(self.dots > 0, "점 찍기가 없습니다");
        assert!(!self.board.get(r, c), "이미 채워진 칸");
        self.dots -= 1;
        self.board.set(r, c);
        self.score += 1;
        let (lines, granted, cleared_mask) = self.resolve_clears();
        let pickups = granted.len() as u32;
        self.cap_check();
        PlaceOutcome {
            cells: 1,
            lines,
            pickups,
            cleared_mask,
            granted,
            dropped: None,
            spawn_due: false,
            hand_empty: self.hand_empty(),
            icon_dropped: false,
            score_delta: 1 + line_score(lines) + pickups * PICKUP_SCORE,
        }
    }

    /// 바꿔 뽑기: 슬롯의 조각을 즉시 다른 조각으로 바꾼다 (가중치 추첨, 같은 조각 가능: 가정 A3).
    pub fn reroll(&mut self, slot: usize, rng: &mut Rng, pw: &PieceWeights) -> u8 {
        assert!(self.rerolls > 0, "바꿔 뽑기가 없습니다");
        assert!(self.hand[slot].is_some(), "빈 슬롯");
        self.rerolls -= 1;
        let p = pw.sample(rng);
        self.hand[slot] = Some(p);
        p
    }

    /// 손패 중 하나라도 놓을 수 있는가.
    pub fn can_place_any(&self) -> bool {
        let mut tried = [false; NUM_PIECES];
        for h in self.hand.iter().flatten() {
            if tried[*h as usize] {
                continue;
            }
            tried[*h as usize] = true;
            if self.board.fits(piece(*h)) {
                return true;
            }
        }
        false
    }

    /// 종료 조건: 회전·반전해도 놓을 곳이 없고 보유 능력이 0개.
    pub fn is_stuck(&self) -> bool {
        !self.hand_empty() && !self.can_place_any() && self.held() == 0
    }

    pub fn check_over(&mut self) {
        if self.score >= SCORE_CAP || self.is_stuck() {
            self.over = true;
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("state json")
    }

    pub fn from_json(s: &str) -> Result<State, String> {
        serde_json::from_str(s).map_err(|e| e.to_string())
    }
}
