//! 한글 모아모아 엔진.
//!
//! 규칙은 이 크레이트 한 곳에만 둔다. 네이티브 학습 바이너리와 WASM 바인딩이 같은 코드를 쓴다.
//! 규칙의 출처는 `docs/DESIGN.md` 2장(이벤트 페이지 원문 기준)이다.

pub mod board;
pub mod eval;
pub mod pieces;
pub mod rng;
pub mod search;
pub mod sim;
pub mod state;

pub use board::{Board, FULL, H, W};
pub use eval::{value_fast, value_full, Weights, FEATURE_NAMES, NF};
pub use pieces::{piece, pieces, Orient, Piece, NUM_PIECES};
pub use rng::Rng;
pub use search::{lookahead, reroll_analysis, reroll_option, search, Move, Plan, RerollOption, RerollReport, SearchParams};
pub use sim::{play_game, policy_step, record_events, summarize, Event, GameResult, PlayConfig, Summary};
pub use state::{
    line_score, AbilityKind, Icon, PieceWeights, PlaceOutcome, State, ABILITY_CAP, ICON_CAP,
    PICKUP_SCORE, P_DOT, SCORE_CAP, SPAWN_EVERY,
};
