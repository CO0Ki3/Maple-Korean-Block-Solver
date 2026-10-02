//! N-tuple 네트워크: 판의 국소 패턴(창)별 조회표. 선형 평가 함수의 잔차를 TD 학습으로 맞춘다.
//!
//! - 창 모양마다 테이블 하나를 모든 위치가 공유한다 (평행 이동 불변).
//! - 테이블 값은 f32 비트를 담은 AtomicU32 라서 여러 스레드가 Hogwild 방식으로 동시에 갱신할 수 있다.
//! - 값의 단위는 점수(점)이며, 평가 함수의 다른 항과 더해진다.

use crate::board::{Board, H, W};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU32, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Shape {
    pub h: u8,
    pub w: u8,
}

pub struct NTuple {
    pub shapes: Vec<Shape>,
    /// 위치별 테이블을 쓸지 (false면 같은 모양의 모든 위치가 테이블 하나를 공유).
    pub per_position: bool,
    offsets: Vec<usize>,
    tables: Vec<AtomicU32>,
    /// 전역 바이어스 (평균 미래 점수 같은 공통 오프셋을 패턴이 아니라 여기서 흡수한다).
    bias: AtomicU32,
    /// 한 판 평가에 쓰이는 조회 횟수 (학습률 정규화용).
    pub active: usize,
}

impl std::fmt::Debug for NTuple {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "NTuple(shapes={:?}, entries={})", self.shapes, self.tables.len())
    }
}

#[derive(Serialize, Deserialize)]
struct Dto {
    shapes: Vec<Shape>,
    tables: Vec<f32>,
    #[serde(default)]
    bias: f32,
    #[serde(default)]
    per_position: bool,
}

impl NTuple {
    /// 기본 모양: 가로줄 전체(1×10), 3×3, 5×2, 2×5.
    pub fn default_shapes() -> Vec<Shape> {
        vec![Shape { h: 1, w: 10 }, Shape { h: 3, w: 3 }, Shape { h: 5, w: 2 }, Shape { h: 2, w: 5 }]
    }

    pub fn new(shapes: Vec<Shape>) -> NTuple {
        NTuple::with_mode(shapes, false)
    }

    pub fn with_mode(shapes: Vec<Shape>, per_position: bool) -> NTuple {
        let mut offsets = Vec::with_capacity(shapes.len());
        let mut total = 0usize;
        let mut active = 0usize;
        for s in &shapes {
            assert!(s.h as usize <= H && s.w as usize <= W && (s.h as usize * s.w as usize) <= 20, "창이 너무 큽니다: {:?}", s);
            offsets.push(total);
            let positions = (H - s.h as usize + 1) * (W - s.w as usize + 1);
            let size = 1usize << (s.h as usize * s.w as usize);
            total += if per_position { size * positions } else { size };
            active += positions;
        }
        let tables = (0..total).map(|_| AtomicU32::new(0f32.to_bits())).collect();
        NTuple { shapes, per_position, offsets, tables, bias: AtomicU32::new(0f32.to_bits()), active }
    }

    pub fn bias(&self) -> f32 {
        f32::from_bits(self.bias.load(Ordering::Relaxed))
    }

    pub fn add_bias(&self, d: f32) {
        let v = self.bias() + d;
        self.bias.store(v.to_bits(), Ordering::Relaxed);
    }

    pub fn len(&self) -> usize {
        self.tables.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tables.is_empty()
    }

    #[inline]
    fn get(&self, i: usize) -> f32 {
        f32::from_bits(self.tables[i].load(Ordering::Relaxed))
    }

    #[inline]
    fn add(&self, i: usize, d: f32) {
        let v = self.get(i) + d;
        self.tables[i].store(v.to_bits(), Ordering::Relaxed);
    }

    /// 창별 패턴 인덱스를 전부 순회한다.
    #[inline]
    fn for_each_index(&self, b: &Board, mut f: impl FnMut(usize)) {
        for (si, s) in self.shapes.iter().enumerate() {
            let (h, w) = (s.h as usize, s.w as usize);
            let mask = ((1u32 << w) - 1) as u16;
            let size = 1usize << (h * w);
            let cols = W - w + 1;
            let base = self.offsets[si];
            for r in 0..=(H - h) {
                for c in 0..cols {
                    let mut idx = 0usize;
                    for i in 0..h {
                        idx |= (((b.rows[r + i] >> c) & mask) as usize) << (i * w);
                    }
                    let pos_base = if self.per_position { base + (r * cols + c) * size } else { base };
                    f(pos_base + idx);
                }
            }
        }
    }

    /// 판의 가치 (바이어스 + 조회표 합).
    pub fn eval(&self, b: &Board) -> f32 {
        let mut v = self.bias();
        self.for_each_index(b, |i| v += self.get(i));
        v
    }

    /// TD 갱신: 활성 항목마다 `delta`를 더한다 (호출자가 학습률/정규화를 적용해 넘긴다).
    pub fn update(&self, b: &Board, delta: f32) {
        self.for_each_index(b, |i| self.add(i, delta));
    }

    pub fn to_json(&self) -> String {
        let tables: Vec<f32> = (0..self.tables.len()).map(|i| self.get(i)).collect();
        serde_json::to_string(&Dto { shapes: self.shapes.clone(), tables, bias: self.bias(), per_position: self.per_position }).unwrap()
    }

    pub fn from_json(s: &str) -> Result<NTuple, String> {
        let d: Dto = serde_json::from_str(s).map_err(|e| e.to_string())?;
        let nt = NTuple::with_mode(d.shapes, d.per_position);
        if d.tables.len() != nt.len() {
            return Err(format!("테이블 길이 불일치: {} vs {}", d.tables.len(), nt.len()));
        }
        for (i, v) in d.tables.iter().enumerate() {
            nt.tables[i].store(v.to_bits(), Ordering::Relaxed);
        }
        nt.bias.store(d.bias.to_bits(), Ordering::Relaxed);
        Ok(nt)
    }

    /// 통계 (진단용): 0이 아닌 항목 수, 절댓값 평균.
    pub fn stats(&self) -> (usize, f32) {
        let mut nz = 0usize;
        let mut sum = 0f64;
        for i in 0..self.tables.len() {
            let v = self.get(i);
            if v != 0.0 {
                nz += 1;
                sum += v.abs() as f64;
            }
        }
        (nz, if nz > 0 { (sum / nz as f64) as f32 } else { 0.0 })
    }
}
