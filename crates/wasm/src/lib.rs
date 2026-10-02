//! 브라우저 바인딩. 모든 입출력은 JSON 문자열 (구조는 engine의 serde 표현 그대로).

use moamoa_engine::{
    eval, lookahead, pieces, play_game, policy_step, record_events, reroll_analysis, reroll_option, search, summarize, Event,
    GameResult, Move, PieceWeights, PlayConfig, Rng, SearchParams, State, Weights, FEATURE_NAMES, SCORE_CAP,
};
use serde::Serialize;
use wasm_bindgen::prelude::*;

fn err(e: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&e.to_string())
}

fn parse_params(s: &str) -> Result<SearchParams, JsValue> {
    if s.trim().is_empty() {
        return Ok(SearchParams::default());
    }
    let v: serde_json::Value = serde_json::from_str(s).map_err(err)?;
    let d = SearchParams::default();
    let g = |k: &str| v.get(k).and_then(|x| x.as_f64());
    Ok(SearchParams {
        beam: g("beam").map(|x| x as usize).unwrap_or(d.beam),
        max_dots: g("max_dots").map(|x| x as u8).unwrap_or(d.max_dots),
        leaf_k: g("leaf_k").map(|x| x as usize).unwrap_or(d.leaf_k),
        samples: g("samples").map(|x| x as usize).unwrap_or(d.samples),
        sample_beam: g("sample_beam").map(|x| x as usize).unwrap_or(d.sample_beam),
        alts: g("alts").map(|x| x as usize).unwrap_or(d.alts),
        death_penalty: g("death_penalty").map(|x| x as f32).unwrap_or(d.death_penalty),
        stuck_penalty: g("stuck_penalty").map(|x| x as f32).unwrap_or(d.stuck_penalty),
    })
}

fn parse_weights(s: &str) -> Result<Weights, JsValue> {
    if s.trim().is_empty() {
        Ok(Weights::default())
    } else {
        Weights::from_json(s).map_err(err)
    }
}

fn parse_pw(s: &str) -> Result<PieceWeights, JsValue> {
    if s.trim().is_empty() {
        return Ok(PieceWeights::uniform());
    }
    let v: serde_json::Value = serde_json::from_str(s).map_err(err)?;
    let arr = match &v {
        serde_json::Value::Array(a) => a.clone(),
        serde_json::Value::Object(m) => m.get("w").and_then(|x| x.as_array()).cloned().ok_or_else(|| err("조각 가중치는 배열이어야 합니다"))?,
        _ => return Err(err("조각 가중치 형식 오류")),
    };
    if arr.len() != moamoa_engine::NUM_PIECES {
        return Err(err(format!("조각 가중치 길이는 {}이어야 합니다", moamoa_engine::NUM_PIECES)));
    }
    let mut pw = PieceWeights::uniform();
    for (i, x) in arr.iter().enumerate() {
        pw.w[i] = x.as_f64().unwrap_or(0.0).max(0.0);
    }
    Ok(pw)
}

#[derive(Serialize)]
struct OrientDto {
    h: u8,
    w: u8,
    rot: u8,
    flip: bool,
    cells: Vec<(u8, u8)>,
}

#[derive(Serialize)]
struct PieceDto {
    id: u8,
    name: &'static str,
    n: u8,
    cells: Vec<(u8, u8)>,
    orients: Vec<OrientDto>,
}

/// 19종 조각과 각 방향의 칸 목록.
#[wasm_bindgen]
pub fn pieces_json() -> String {
    let v: Vec<PieceDto> = pieces()
        .iter()
        .map(|p| PieceDto {
            id: p.id,
            name: p.name,
            n: p.n,
            cells: p.cells.clone(),
            orients: p.orients.iter().map(|o| OrientDto { h: o.h, w: o.w, rot: o.rot, flip: o.flip, cells: o.cells.clone() }).collect(),
        })
        .collect();
    serde_json::to_string(&v).unwrap()
}

#[wasm_bindgen]
pub fn default_weights_json() -> String {
    Weights::default().to_json()
}

#[wasm_bindgen]
pub fn feature_names_json() -> String {
    serde_json::to_string(&FEATURE_NAMES).unwrap()
}

#[wasm_bindgen]
pub fn default_params_json() -> String {
    serde_json::to_string(&SearchParams::default()).unwrap()
}

/// 새 게임 상태 (손패 3개 추첨).
#[wasm_bindgen]
pub fn new_game_json(seed: u32, pw_json: &str) -> Result<String, JsValue> {
    let pw = parse_pw(pw_json)?;
    let mut rng = Rng::new(seed as u64);
    Ok(State::new_game(&mut rng, &pw).to_json())
}

/// 추천 계획.
#[wasm_bindgen]
pub fn solve_json(state_json: &str, params_json: &str, weights_json: &str, pw_json: &str) -> Result<String, JsValue> {
    let st = State::from_json(state_json).map_err(err)?;
    let p = parse_params(params_json)?;
    let w = parse_weights(weights_json)?;
    let pw = parse_pw(pw_json)?;
    let plan = search(&st, &pw, &w, &p);
    serde_json::to_string(&plan).map_err(err)
}

/// 바꿔 뽑기 기대값 분석.
#[wasm_bindgen]
pub fn reroll_json(state_json: &str, params_json: &str, weights_json: &str, pw_json: &str) -> Result<String, JsValue> {
    let st = State::from_json(state_json).map_err(err)?;
    let p = parse_params(params_json)?;
    let w = parse_weights(weights_json)?;
    let pw = parse_pw(pw_json)?;
    let rep = reroll_analysis(&st, &pw, &w, &p);
    serde_json::to_string(&rep).map_err(err)
}

/// 한 슬롯만 바꿔 뽑기 분석 (워커 분할용). 빈 슬롯이면 null.
#[wasm_bindgen]
pub fn reroll_slot_json(state_json: &str, params_json: &str, weights_json: &str, pw_json: &str, slot: u32) -> Result<String, JsValue> {
    let st = State::from_json(state_json).map_err(err)?;
    let p = parse_params(params_json)?;
    let w = parse_weights(weights_json)?;
    let pw = parse_pw(pw_json)?;
    let opt = reroll_option(&st, &pw, &w, &p, slot as usize);
    serde_json::to_string(&opt).map_err(err)
}

/// 손패를 다 놓은 상태에서 다음 손패를 샘플링한 평균 가치. 후보 계획을 워커에 나눠 재평가할 때 쓴다.
#[wasm_bindgen]
pub fn lookahead_json(state_json: &str, params_json: &str, weights_json: &str, pw_json: &str, seed: u32) -> Result<String, JsValue> {
    let st = State::from_json(state_json).map_err(err)?;
    let p = parse_params(params_json)?;
    let w = parse_weights(weights_json)?;
    let pw = parse_pw(pw_json)?;
    let v = lookahead(&st, &pw, &w, &p, seed as u64);
    serde_json::to_string(&serde_json::json!({ "value": v, "samples": p.samples })).map_err(err)
}

#[derive(Serialize)]
struct ApplyResult {
    state: State,
    outcome: Option<moamoa_engine::PlaceOutcome>,
    new_piece: Option<u8>,
}

/// 행동 적용. 배치·점 찍기는 결정적 전이만 수행한다 (아이콘 생성·새 손패는 실제 게임 화면에서 읽는다).
/// `simulate=true`면 난수로 아이콘 생성·손패 추첨·바꿔 뽑기 결과까지 흉내 낸다.
#[wasm_bindgen]
pub fn apply_move_json(state_json: &str, move_json: &str, simulate: bool, seed: u32, pw_json: &str) -> Result<String, JsValue> {
    let mut st = State::from_json(state_json).map_err(err)?;
    let mv: Move = serde_json::from_str(move_json).map_err(err)?;
    let pw = parse_pw(pw_json)?;
    let mut rng = Rng::new(seed as u64);
    let mut outcome = None;
    let mut new_piece = None;
    match mv {
        Move::Place { slot, orient, r, c } => {
            let (slot, orient, r, c) = (slot as usize, orient as usize, r as usize, c as usize);
            let pid = st.hand.get(slot).copied().flatten().ok_or_else(|| err("빈 슬롯"))?;
            let o = moamoa_engine::piece(pid).orients.get(orient).ok_or_else(|| err("방향 인덱스 오류"))?;
            if r + o.h as usize > moamoa_engine::H || c + o.w as usize > moamoa_engine::W || !st.board.can_place(o, r, c) {
                return Err(err("놓을 수 없는 위치"));
            }
            outcome = Some(if simulate { st.place(slot, orient, r, c, &mut rng, &pw) } else { st.place_det(slot, orient, r, c) });
        }
        Move::Dot { r, c } => {
            if st.dots == 0 {
                return Err(err("점 찍기가 없습니다"));
            }
            if st.board.get(r as usize, c as usize) {
                return Err(err("이미 채워진 칸"));
            }
            outcome = Some(st.dot(r as usize, c as usize));
            st.check_over();
        }
        Move::Reroll { slot } => {
            if st.rerolls == 0 || st.hand[slot as usize].is_none() {
                return Err(err("바꿔 뽑기를 쓸 수 없습니다"));
            }
            if simulate {
                new_piece = Some(st.reroll(slot as usize, &mut rng, &pw));
            } else {
                st.rerolls -= 1;
                st.hand[slot as usize] = None;
            }
        }
    }
    serde_json::to_string(&ApplyResult { state: st, outcome, new_piece }).map_err(err)
}

/// 특징 벡터와 가치 (디버그·튜닝 UI용).
#[wasm_bindgen]
pub fn evaluate_json(state_json: &str, weights_json: &str, pw_json: &str) -> Result<String, JsValue> {
    let st = State::from_json(state_json).map_err(err)?;
    let w = parse_weights(weights_json)?;
    let pw = parse_pw(pw_json)?;
    let f = eval::features_all(&st, &pw);
    let v = eval::value_full(&st, &pw, &w);
    let names: Vec<(&str, f32, f32)> = FEATURE_NAMES.iter().zip(f.iter()).zip(w.w.iter()).map(|((n, f), w)| (*n, *f, *w)).collect();
    serde_json::to_string(&serde_json::json!({ "value": v, "features": names, "stuck": st.is_stuck(), "can_place_any": st.can_place_any() })).map_err(err)
}

/// 상태 JSON 검증 + 정규화 (UI가 만든 상태가 규칙에 맞는지).
#[wasm_bindgen]
pub fn normalize_state_json(state_json: &str) -> Result<String, JsValue> {
    let mut st = State::from_json(state_json).map_err(err)?;
    st.check_over();
    Ok(st.to_json())
}

// ===== 시뮬레이터 =====

#[derive(Serialize, serde::Deserialize)]
struct RngDto {
    s: [String; 4],
}

fn rng_to_dto(r: &Rng) -> RngDto {
    let st = r.state();
    RngDto { s: [st[0].to_string(), st[1].to_string(), st[2].to_string(), st[3].to_string()] }
}

fn rng_from_json(s: &str) -> Result<Rng, JsValue> {
    let d: RngDto = serde_json::from_str(s).map_err(err)?;
    let mut st = [0u64; 4];
    for i in 0..4 {
        st[i] = d.s[i].parse::<u64>().map_err(err)?;
    }
    Ok(Rng::from_state(st))
}

fn parse_cfg(s: &str) -> Result<PlayConfig, JsValue> {
    if s.trim().is_empty() {
        Ok(PlayConfig::default())
    } else {
        serde_json::from_str(s).map_err(err)
    }
}

#[derive(Serialize)]
struct SimNew {
    state: State,
    rng: RngDto,
}

/// 새 시뮬레이션 게임: 손패 3개를 뽑은 상태와 난수 상태.
#[wasm_bindgen]
pub fn sim_new_json(seed: u32, pw_json: &str) -> Result<String, JsValue> {
    let pw = parse_pw(pw_json)?;
    let mut rng = Rng::new(seed as u64);
    let st = State::new_game(&mut rng, &pw);
    serde_json::to_string(&SimNew { state: st, rng: rng_to_dto(&rng) }).map_err(err)
}

#[derive(Serialize)]
struct SimStep {
    state: State,
    rng: RngDto,
    events: Vec<Event>,
}

/// 정책 한 수 (네이티브 자기대전과 같은 `policy_step`).
#[wasm_bindgen]
pub fn sim_step_json(state_json: &str, rng_json: &str, params_json: &str, weights_json: &str, pw_json: &str, cfg_json: &str) -> Result<String, JsValue> {
    let mut st = State::from_json(state_json).map_err(err)?;
    let mut rng = rng_from_json(rng_json)?;
    let p = parse_params(params_json)?;
    let w = parse_weights(weights_json)?;
    let pw = parse_pw(pw_json)?;
    let cfg = parse_cfg(cfg_json)?;
    let events = policy_step(&mut st, &mut rng, &pw, &w, &p, &cfg);
    serde_json::to_string(&SimStep { state: st, rng: rng_to_dto(&rng), events }).map_err(err)
}

#[derive(Serialize)]
struct SimGames {
    results: Vec<GameResult>,
    summary: moamoa_engine::Summary,
}

/// 여러 판을 끝까지 둔 통계 (워커에서 호출, 호출자가 시드 범위를 나눈다).
#[wasm_bindgen]
pub fn sim_games_json(games: u32, seed: u32, params_json: &str, weights_json: &str, pw_json: &str, cfg_json: &str) -> Result<String, JsValue> {
    let p = parse_params(params_json)?;
    let w = parse_weights(weights_json)?;
    let pw = parse_pw(pw_json)?;
    let cfg = parse_cfg(cfg_json)?;
    let results: Vec<GameResult> = (0..games).map(|k| play_game(seed as u64 + k as u64, &pw, &w, &p, &cfg)).collect();
    let summary = summarize(&results);
    serde_json::to_string(&SimGames { results, summary }).map_err(err)
}

/// 이벤트 목록을 GameResult 통계에 더한다 (브라우저가 단계별 통계를 유지할 때).
#[wasm_bindgen]
pub fn sim_record_json(result_json: &str, events_json: &str) -> Result<String, JsValue> {
    let mut res: GameResult = serde_json::from_str(result_json).map_err(err)?;
    let events: Vec<Event> = serde_json::from_str(events_json).map_err(err)?;
    record_events(&mut res, &events);
    serde_json::to_string(&res).map_err(err)
}

#[wasm_bindgen]
pub fn score_cap() -> u32 {
    SCORE_CAP
}
