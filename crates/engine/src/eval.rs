//! 평가 함수: 상태 특징 × 학습된 가중치. 값의 단위는 점수(점)와 같다.
//!
//! `fast` 특징은 탐색 내부 노드마다, `slow` 특징(조각 분포 기반 커버리지)은 리프에서만 계산한다.

use crate::board::{Board, FULL, H, W};
use crate::pieces::pieces;
use crate::state::{PieceWeights, State, ABILITY_CAP};
use serde::{Deserialize, Serialize};

pub const NF: usize = 24;
pub const N_FAST: usize = 22;

pub const FEATURE_NAMES: [&str; NF] = [
    "filled",            // 0  채워진 칸 수
    "nonempty_rows",     // 1  비어 있지 않은 줄 수
    "row_trans",         // 2  가로 전이 (테두리는 채움으로 취급)
    "col_trans",         // 3  세로 전이 (위·아래 테두리 채움)
    "isolated",          // 4  사방이 막힌 빈 칸
    "narrow",            // 5  좌우가 막힌 빈 칸 (폭 1 통로)
    "rows_missing_1",    // 6  빈 칸이 1개인 줄
    "rows_missing_2",    // 7  빈 칸이 2개인 줄
    "rows_missing_3",    // 8  빈 칸이 3개인 줄
    "well_max",          // 9  한 열만 비어 있는 줄이 연속되는 최대 길이 (ㅣ로 다중 제거)
    "well_sq",           // 10 그런 연속 구간의 길이² 합
    "pair_near_full",    // 11 이웃한 두 줄이 모두 빈 칸 ≤ 2
    "empty_components",  // 12 빈 칸 연결 성분 수
    "small_components",  // 13 크기 ≤ 2인 빈 성분 수
    "icon_count",        // 14 판 위 아이콘 수
    "icon_row_missing",  // 15 (보유 < 7일 때) 아이콘 줄의 빈 칸 수 합
    "dots",              // 16 보유 점 찍기
    "rerolls",           // 17 보유 바꿔 뽑기
    "held_sqrt",         // 18 sqrt(보유 합계)
    "at_cap",            // 19 보유 7개 (생성 차단)
    "max_row_fill",      // 20 가장 많이 찬 줄의 칸 수
    "edge_empty",        // 21 맨 위/아래 줄의 빈 칸 수 (판 끝은 메우기 어렵다)
    "fit_risk",          // 22 [slow] 놓을 자리가 없는 조각의 확률 합
    "dead_coverage",     // 23 [slow] 빈 칸별 max(0, 1 − 덮을 수 있는 조각 확률 합)
];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Weights {
    pub w: [f32; NF],
}

impl Default for Weights {
    /// 손으로 맞춘 초기값. 학습(CEM)의 출발점이다.
    fn default() -> Self {
        let mut w = [0f32; NF];
        w[0] = -6.0;
        w[1] = -10.0;
        w[2] = -12.0;
        w[3] = -12.0;
        w[4] = -250.0;
        w[5] = -20.0;
        w[6] = 60.0;
        w[7] = 25.0;
        w[8] = 8.0;
        w[9] = 40.0;
        w[10] = 40.0;
        w[11] = 30.0;
        w[12] = -40.0;
        w[13] = -200.0;
        w[14] = 0.0;
        w[15] = -15.0;
        w[16] = 200.0;
        w[17] = 250.0;
        w[18] = 300.0;
        w[19] = -100.0;
        w[20] = 0.0;
        w[21] = -5.0;
        w[22] = -6000.0;
        w[23] = -250.0;
        Weights { w }
    }
}

impl Weights {
    pub fn to_json(&self) -> String {
        let map: serde_json::Map<String, serde_json::Value> = FEATURE_NAMES
            .iter()
            .zip(self.w.iter())
            .map(|(n, v)| (n.to_string(), serde_json::json!(v)))
            .collect();
        serde_json::to_string_pretty(&map).unwrap()
    }

    /// 이름→값 객체 또는 배열 둘 다 받는다. 없는 이름은 기본값 유지.
    pub fn from_json(s: &str) -> Result<Weights, String> {
        let v: serde_json::Value = serde_json::from_str(s).map_err(|e| e.to_string())?;
        let mut w = Weights::default();
        match v {
            serde_json::Value::Array(a) => {
                if a.len() != NF {
                    return Err(format!("가중치 배열 길이가 {}이 아닙니다: {}", NF, a.len()));
                }
                for (i, x) in a.iter().enumerate() {
                    w.w[i] = x.as_f64().ok_or("숫자가 아닙니다")? as f32;
                }
            }
            serde_json::Value::Object(m) => {
                if let Some(inner) = m.get("w") {
                    return Weights::from_json(&inner.to_string());
                }
                for (i, n) in FEATURE_NAMES.iter().enumerate() {
                    if let Some(x) = m.get(*n) {
                        w.w[i] = x.as_f64().ok_or(format!("{}가 숫자가 아닙니다", n))? as f32;
                    }
                }
            }
            _ => return Err("가중치 JSON 형식이 아닙니다".into()),
        }
        Ok(w)
    }
}

/// 빈 칸의 4-연결 성분: (성분 수, 크기 ≤ 2인 성분 수). 행 구간 단위 union-find.
fn empty_components(rows: &[u16; H]) -> (u32, u32) {
    const MAXSEG: usize = 80;
    let mut seg = [0u16; MAXSEG];
    let mut parent = [0u8; MAXSEG];
    let mut size = [0u16; MAXSEG];
    let mut n = 0usize;
    let (mut ps, mut pe) = (0usize, 0usize);
    fn find(parent: &mut [u8; MAXSEG], mut i: usize) -> usize {
        while parent[i] as usize != i {
            let p = parent[i] as usize;
            parent[i] = parent[p];
            i = parent[i] as usize;
        }
        i
    }
    for r in 0..H {
        let mut e = !rows[r] & FULL;
        let start = n;
        while e != 0 {
            let lo = e & e.wrapping_neg();
            let run = e & !(e.wrapping_add(lo));
            seg[n] = run;
            parent[n] = n as u8;
            size[n] = run.count_ones() as u16;
            for p in ps..pe {
                if seg[p] & run != 0 {
                    let a = find(&mut parent, p);
                    let b = find(&mut parent, n);
                    if a != b {
                        parent[a] = b as u8;
                        size[b] += size[a];
                    }
                }
            }
            n += 1;
            e &= !run;
        }
        ps = start;
        pe = n;
    }
    let (mut count, mut small) = (0, 0);
    for i in 0..n {
        if parent[i] as usize == i {
            count += 1;
            if size[i] <= 2 {
                small += 1;
            }
        }
    }
    (count, small)
}

pub fn features_fast(st: &State, f: &mut [f32; NF]) {
    let rows = &st.board.rows;
    let (mut filled, mut nonempty, mut row_trans, mut col_trans) = (0u32, 0u32, 0u32, 0u32);
    let (mut isolated, mut narrow, mut m1, mut m2, mut m3) = (0u32, 0u32, 0u32, 0u32, 0u32);
    let mut max_fill = 0u32;
    let mut miss = [0u32; H];
    let mut prev = FULL;
    for r in 0..H {
        let x = rows[r];
        let fc = x.count_ones();
        filled += fc;
        max_fill = max_fill.max(fc);
        if fc > 0 {
            nonempty += 1;
        }
        let ms = W as u32 - fc;
        miss[r] = ms;
        match ms {
            1 => m1 += 1,
            2 => m2 += 1,
            3 => m3 += 1,
            _ => {}
        }
        let y = ((x as u32) << 1) | 1 | (1 << (W + 1));
        row_trans += ((y ^ (y >> 1)) & ((1 << (W + 1)) - 1)).count_ones();
        let dn = if r + 1 < H { rows[r + 1] } else { FULL };
        col_trans += (x ^ prev).count_ones();
        let e = !x & FULL;
        let l = ((x << 1) | 1) & FULL;
        let rr = (x >> 1) | (1 << (W - 1));
        let nar = e & l & rr;
        narrow += nar.count_ones();
        isolated += (nar & prev & dn).count_ones();
        prev = x;
    }
    col_trans += (prev ^ FULL).count_ones();

    let (mut well_max, mut well_sq) = (0u32, 0u32);
    for c in 0..W {
        let target = FULL & !(1 << c);
        let mut run = 0u32;
        for r in 0..=H {
            if r < H && rows[r] == target {
                run += 1;
            } else if run > 0 {
                let k = run.min(5);
                well_sq += k * k;
                well_max = well_max.max(run);
                run = 0;
            }
        }
    }
    let mut pair = 0u32;
    for r in 0..H - 1 {
        if miss[r] <= 2 && miss[r + 1] <= 2 {
            pair += 1;
        }
    }
    let (comps, small) = empty_components(rows);

    let held = st.held();
    let mut icon_missing = 0u32;
    if held < ABILITY_CAP {
        for ic in st.icons.iter() {
            icon_missing += miss[ic.r as usize];
        }
    }
    let edge_empty = (W as u32 - rows[0].count_ones()) + (W as u32 - rows[H - 1].count_ones());

    f[0] = filled as f32;
    f[1] = nonempty as f32;
    f[2] = row_trans as f32;
    f[3] = col_trans as f32;
    f[4] = isolated as f32;
    f[5] = narrow as f32;
    f[6] = m1 as f32;
    f[7] = m2 as f32;
    f[8] = m3 as f32;
    f[9] = well_max as f32;
    f[10] = well_sq as f32;
    f[11] = pair as f32;
    f[12] = comps as f32;
    f[13] = small as f32;
    f[14] = st.icons.len() as f32;
    f[15] = icon_missing as f32;
    f[16] = st.dots as f32;
    f[17] = st.rerolls as f32;
    f[18] = (held as f32).sqrt();
    f[19] = if held >= ABILITY_CAP { 1.0 } else { 0.0 };
    f[20] = max_fill as f32;
    f[21] = edge_empty as f32;
}

/// 조각 분포 기반 특징. 리프에서만.
pub fn features_slow(st: &State, pw: &PieceWeights, f: &mut [f32; NF]) {
    let b = &st.board;
    let probs = pw.probs();
    let mut cov = [0f32; W * H];
    let mut fit_risk = 0f32;
    for p in pieces() {
        let pr = probs[p.id as usize] as f32;
        if pr <= 0.0 {
            continue;
        }
        let mut cb = Board::empty();
        let mut any = false;
        for o in &p.orients {
            let (h, w) = (o.h as usize, o.w as usize);
            for r in 0..=(H - h) {
                for c in 0..=(W - w) {
                    if b.can_place(o, r, c) {
                        any = true;
                        cb.place(o, r, c);
                    }
                }
            }
        }
        if !any {
            fit_risk += pr;
        }
        for r in 0..H {
            let mut m = cb.rows[r];
            while m != 0 {
                let c = m.trailing_zeros() as usize;
                cov[r * W + c] += pr;
                m &= m - 1;
            }
        }
    }
    let mut dead = 0f32;
    for r in 0..H {
        let mut e = !b.rows[r] & FULL;
        while e != 0 {
            let c = e.trailing_zeros() as usize;
            dead += (1.0 - cov[r * W + c]).max(0.0);
            e &= e - 1;
        }
    }
    f[22] = fit_risk;
    f[23] = dead;
}

#[inline]
pub fn value_fast(st: &State, w: &Weights) -> f32 {
    let mut f = [0f32; NF];
    features_fast(st, &mut f);
    let mut v = 0f32;
    for i in 0..N_FAST {
        v += w.w[i] * f[i];
    }
    v
}

pub fn value_full(st: &State, pw: &PieceWeights, w: &Weights) -> f32 {
    let mut f = [0f32; NF];
    features_fast(st, &mut f);
    features_slow(st, pw, &mut f);
    let mut v = 0f32;
    for i in 0..NF {
        v += w.w[i] * f[i];
    }
    v
}

pub fn features_all(st: &State, pw: &PieceWeights) -> [f32; NF] {
    let mut f = [0f32; NF];
    features_fast(st, &mut f);
    features_slow(st, pw, &mut f);
    f
}
