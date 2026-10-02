//! 10×16 비트보드. 행마다 u16 하나(열 c → 비트 c). 행 0이 맨 위.

use crate::pieces::{Orient, Piece};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub const W: usize = 10;
pub const H: usize = 16;
pub const FULL: u16 = (1 << W) - 1;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct Board {
    pub rows: [u16; H],
}

impl Board {
    pub const fn empty() -> Self {
        Board { rows: [0; H] }
    }

    #[inline]
    pub fn get(&self, r: usize, c: usize) -> bool {
        (self.rows[r] >> c) & 1 == 1
    }

    #[inline]
    pub fn set(&mut self, r: usize, c: usize) {
        self.rows[r] |= 1 << c;
    }

    #[inline]
    pub fn clear(&mut self, r: usize, c: usize) {
        self.rows[r] &= !(1 << c);
    }

    /// 조각 방향 `o`의 왼쪽 위 칸을 (r, c)에 맞춰 놓을 수 있는가. 호출자가 경계(r+h ≤ H, c+w ≤ W)를 보장한다.
    #[inline]
    pub fn can_place(&self, o: &Orient, r: usize, c: usize) -> bool {
        let h = o.h as usize;
        for i in 0..h {
            if self.rows[r + i] & (o.rows[i] << c) != 0 {
                return false;
            }
        }
        true
    }

    #[inline]
    pub fn place(&mut self, o: &Orient, r: usize, c: usize) {
        let h = o.h as usize;
        for i in 0..h {
            self.rows[r + i] |= o.rows[i] << c;
        }
    }

    /// 가득 찬 행의 비트마스크 (행 r → 비트 r).
    #[inline]
    pub fn full_mask(&self) -> u16 {
        let mut m = 0u16;
        for r in 0..H {
            if self.rows[r] == FULL {
                m |= 1 << r;
            }
        }
        m
    }

    #[inline]
    pub fn clear_mask(&mut self, m: u16) {
        for r in 0..H {
            if (m >> r) & 1 == 1 {
                self.rows[r] = 0;
            }
        }
    }

    pub fn filled(&self) -> u32 {
        self.rows.iter().map(|x| x.count_ones()).sum()
    }

    pub fn empty_cells(&self) -> u32 {
        (W * H) as u32 - self.filled()
    }

    /// 어떤 방향으로든 놓을 자리가 있는가.
    pub fn fits(&self, p: &Piece) -> bool {
        for o in &p.orients {
            let (h, w) = (o.h as usize, o.w as usize);
            for r in 0..=(H - h) {
                for c in 0..=(W - w) {
                    if self.can_place(o, r, c) {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// 모든 합법 배치를 (방향 인덱스, 행, 열)로 순회한다.
    pub fn for_each_placement(&self, p: &Piece, mut f: impl FnMut(usize, usize, usize)) {
        for (oi, o) in p.orients.iter().enumerate() {
            let (h, w) = (o.h as usize, o.w as usize);
            for r in 0..=(H - h) {
                for c in 0..=(W - w) {
                    if self.can_place(o, r, c) {
                        f(oi, r, c);
                    }
                }
            }
        }
    }

    /// 테스트·직렬화용. 각 문자열은 10글자, '#'가 채움. 16줄 미만이면 위쪽을 빈 줄로 채운다.
    pub fn from_strings(rows: &[impl AsRef<str>]) -> Result<Board, String> {
        if rows.len() > H {
            return Err(format!("행이 {}개를 넘습니다: {}", H, rows.len()));
        }
        let mut b = Board::empty();
        let offset = H - rows.len();
        for (i, s) in rows.iter().enumerate() {
            let s = s.as_ref();
            let chars: Vec<char> = s.chars().collect();
            if chars.len() != W {
                return Err(format!("행 {}의 길이가 {}이 아닙니다: {:?}", i, W, s));
            }
            let mut m = 0u16;
            for (c, ch) in chars.iter().enumerate() {
                match ch {
                    '#' | '1' | 'X' | 'x' | 'O' | 'o' => m |= 1 << c,
                    '.' | '0' | '_' | ' ' => {}
                    other => return Err(format!("알 수 없는 문자 {:?} (행 {})", other, i)),
                }
            }
            b.rows[offset + i] = m;
        }
        Ok(b)
    }

    pub fn to_strings(&self) -> Vec<String> {
        self.rows
            .iter()
            .map(|&x| (0..W).map(|c| if (x >> c) & 1 == 1 { '#' } else { '.' }).collect())
            .collect()
    }
}

impl Serialize for Board {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(self.to_strings())
    }
}

impl<'de> Deserialize<'de> for Board {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v: Vec<String> = Vec::deserialize(d)?;
        Board::from_strings(&v).map_err(serde::de::Error::custom)
    }
}

impl std::fmt::Display for Board {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for s in self.to_strings() {
            writeln!(f, "{}", s)?;
        }
        Ok(())
    }
}
