//! 조각 정의와 방향(회전·반전) 사전 계산.
//!
//! 19종 = 자음 14 + 모음 5 (ㆍ ㅡ ㅣ ㅏ ㅑ). ㅓㅗㅜ는 ㅏ의, ㅕㅛㅠ는 ㅑ의 회전·반전이라 별도 조각이 아니다.
//! 모양은 이전 솔버(block-solver.html)의 BUILTIN 테이블과 동일하며, 지난 이벤트에서 이 19종 외에는 등장하지 않았다.

use std::sync::OnceLock;

pub const NUM_PIECES: usize = 19;
/// 조각의 최대 높이/너비 (ㅎ이 5칸 너비, ㅣ·ㄹ·ㅌ·ㅑ가 5칸 높이).
pub const MAX_H: usize = 5;

pub struct PieceDef {
    pub name: &'static str,
    pub rows: &'static [&'static str],
}

pub static PIECE_DEFS: [PieceDef; NUM_PIECES] = [
    PieceDef { name: "ㄱ", rows: &["##", ".#", ".#"] },
    PieceDef { name: "ㄴ", rows: &["#.", "##"] },
    PieceDef { name: "ㄷ", rows: &["##", "#.", "##"] },
    PieceDef { name: "ㄹ", rows: &["##", ".#", "##", "#.", "##"] },
    PieceDef { name: "ㅁ", rows: &["###", "#.#", "###"] },
    PieceDef { name: "ㅂ", rows: &["#.#", "###", "#.#", "###"] },
    PieceDef { name: "ㅅ", rows: &[".#.", "#.#"] },
    PieceDef { name: "ㅇ", rows: &[".#.", "#.#", ".#."] },
    PieceDef { name: "ㅈ", rows: &["###", ".#.", "#.#"] },
    PieceDef { name: "ㅊ", rows: &[".#.", "###", ".#.", "#.#"] },
    PieceDef { name: "ㅋ", rows: &["##", ".#", "##", ".#"] },
    PieceDef { name: "ㅌ", rows: &["##", "#.", "##", "#.", "##"] },
    PieceDef { name: "ㅍ", rows: &["####", ".##.", "####"] },
    PieceDef { name: "ㅎ", rows: &["..#..", "#####", ".#.#.", "..#.."] },
    PieceDef { name: "ㆍ", rows: &["#"] },
    PieceDef { name: "ㅡ", rows: &["###"] },
    PieceDef { name: "ㅣ", rows: &["#", "#", "#", "#", "#"] },
    PieceDef { name: "ㅏ", rows: &["#.", "##", "#."] },
    PieceDef { name: "ㅑ", rows: &["#.", "##", "#.", "##", "#."] },
];

/// 조각의 한 방향. `rows[i]`는 i번째 행의 비트마스크(열 c → 비트 c).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Orient {
    pub h: u8,
    pub w: u8,
    pub n: u8,
    pub rows: [u16; MAX_H],
    /// 정규화된 (행, 열) 목록, 정렬됨.
    pub cells: Vec<(u8, u8)>,
    /// 이 방향을 처음 만든 경로: 기준 모양을 `flip`(좌우 반전) 후 시계 방향으로 `rot`번 회전.
    pub rot: u8,
    pub flip: bool,
}

#[derive(Clone, Debug)]
pub struct Piece {
    pub id: u8,
    pub name: &'static str,
    pub n: u8,
    pub cells: Vec<(u8, u8)>,
    pub orients: Vec<Orient>,
}

fn normalize(cells: &[(i32, i32)]) -> Vec<(u8, u8)> {
    let mr = cells.iter().map(|p| p.0).min().unwrap();
    let mc = cells.iter().map(|p| p.1).min().unwrap();
    let mut v: Vec<(u8, u8)> = cells
        .iter()
        .map(|p| ((p.0 - mr) as u8, (p.1 - mc) as u8))
        .collect();
    v.sort();
    v
}

/// 시계 방향 90° 회전: (r, c) → (c, -r).
fn rot_cw(cells: &[(i32, i32)]) -> Vec<(i32, i32)> {
    cells.iter().map(|&(r, c)| (c, -r)).collect()
}

/// 좌우 반전: (r, c) → (r, -c).
fn flip_h(cells: &[(i32, i32)]) -> Vec<(i32, i32)> {
    cells.iter().map(|&(r, c)| (r, -c)).collect()
}

fn make_orient(cells: Vec<(u8, u8)>, rot: u8, flip: bool) -> Orient {
    let h = cells.iter().map(|p| p.0).max().unwrap() + 1;
    let w = cells.iter().map(|p| p.1).max().unwrap() + 1;
    let mut rows = [0u16; MAX_H];
    for &(r, c) in &cells {
        rows[r as usize] |= 1 << c;
    }
    Orient { h, w, n: cells.len() as u8, rows, cells, rot, flip }
}

pub fn cells_from_rows(rows: &[&str]) -> Vec<(i32, i32)> {
    rows.iter()
        .enumerate()
        .flat_map(|(r, s)| {
            s.chars()
                .enumerate()
                .filter(|(_, ch)| *ch == '#')
                .map(move |(c, _)| (r as i32, c as i32))
        })
        .collect()
}

/// 임의의 칸 목록에서 중복 없는 방향 목록을 만든다 (UI의 사용자 정의 조각에도 쓸 수 있다).
pub fn orientations_of(base: &[(i32, i32)]) -> Vec<Orient> {
    let mut orients = Vec::new();
    let mut seen: Vec<Vec<(u8, u8)>> = Vec::new();
    for flip in [false, true] {
        let mut cur = if flip { flip_h(base) } else { base.to_vec() };
        for rot in 0..4u8 {
            let n = normalize(&cur);
            if !seen.contains(&n) {
                seen.push(n.clone());
                orients.push(make_orient(n, rot, flip));
            }
            cur = rot_cw(&cur);
        }
    }
    orients
}

fn build() -> Vec<Piece> {
    PIECE_DEFS
        .iter()
        .enumerate()
        .map(|(id, def)| {
            let base = cells_from_rows(def.rows);
            let orients = orientations_of(&base);
            let cells = normalize(&base);
            Piece { id: id as u8, name: def.name, n: cells.len() as u8, cells, orients }
        })
        .collect()
}

pub fn pieces() -> &'static [Piece] {
    static P: OnceLock<Vec<Piece>> = OnceLock::new();
    P.get_or_init(build)
}

#[inline]
pub fn piece(id: u8) -> &'static Piece {
    &pieces()[id as usize]
}

pub fn piece_by_name(name: &str) -> Option<&'static Piece> {
    pieces().iter().find(|p| p.name == name)
}
