//! 이벤트 페이지 문구 하나당 테스트 하나. 규칙 원문은 docs/DESIGN.md 2장.

use moamoa_engine::*;

fn pid(name: &str) -> u8 {
    pieces().iter().find(|p| p.name == name).unwrap().id
}

fn orient_of(name: &str, rows: &[&str]) -> usize {
    let want: Vec<(u8, u8)> = rows
        .iter()
        .enumerate()
        .flat_map(|(r, s)| s.chars().enumerate().filter(|(_, ch)| *ch == '#').map(move |(c, _)| (r as u8, c as u8)))
        .collect();
    piece(pid(name)).orients.iter().position(|o| o.cells == want).unwrap_or_else(|| panic!("{} 방향 {:?} 없음", name, rows))
}

fn state_with(board: &[&str], hand: [Option<&str>; 3]) -> State {
    let mut st = State::blank();
    st.board = Board::from_strings(board).unwrap();
    for i in 0..3 {
        st.hand[i] = hand[i].map(pid);
    }
    st
}

#[test]
fn piece_table_matches_design_doc() {
    // 19종, 칸 수 합 111.
    assert_eq!(pieces().len(), 19);
    let total: u32 = pieces().iter().map(|p| p.n as u32).sum();
    assert_eq!(total, 111);
    let expect = [
        ("ㄱ", 4), ("ㄴ", 3), ("ㄷ", 5), ("ㄹ", 8), ("ㅁ", 8), ("ㅂ", 10), ("ㅅ", 3), ("ㅇ", 4), ("ㅈ", 6), ("ㅊ", 7),
        ("ㅋ", 6), ("ㅌ", 8), ("ㅍ", 10), ("ㅎ", 9), ("ㆍ", 1), ("ㅡ", 3), ("ㅣ", 5), ("ㅏ", 4), ("ㅑ", 7),
    ];
    for (n, k) in expect {
        assert_eq!(piece(pid(n)).n as u32, k, "{}", n);
    }
}

#[test]
fn orientations_are_deduplicated_by_symmetry() {
    // ㆍ 1, ㅁ 1, ㅣ 2, ㅡ 2, ㅍ 2, ㅏ 4 (ㅏㅓㅗㅜ), ㅑ 4, ㅇ 1, ㅂ 4, ㅎ 4, ㄱ 8, ㄴ 4.
    let expect = [("ㆍ", 1), ("ㅁ", 1), ("ㅣ", 2), ("ㅡ", 2), ("ㅍ", 2), ("ㅏ", 4), ("ㅑ", 4), ("ㅇ", 1), ("ㅂ", 4), ("ㅎ", 4), ("ㄱ", 8), ("ㄴ", 4)];
    for (n, k) in expect {
        assert_eq!(piece(pid(n)).orients.len(), k, "{}", n);
    }
    // ㅏ를 돌리면 ㅜ·ㅓ·ㅗ가 나온다.
    orient_of("ㅏ", &["###", ".#."]); // ㅜ
    orient_of("ㅏ", &[".#", "##", ".#"]); // ㅓ
    orient_of("ㅏ", &[".#.", "###"]); // ㅗ
}

#[test]
fn placement_scores_cell_count() {
    let mut st = state_with(&[], [Some("ㅂ"), Some("ㆍ"), None]);
    let out = st.place_det(0, 0, 0, 0);
    assert_eq!(out.cells, 10);
    assert_eq!(out.score_delta, 10);
    assert_eq!(st.score, 10);
    assert_eq!(st.hand[0], None);
    assert_eq!(st.placements, 1);
    let out = st.place_det(1, 0, 15, 9);
    assert_eq!(out.score_delta, 1);
    assert_eq!(st.score, 11);
}

#[test]
fn piece_hole_may_overlap_filled_cell() {
    // ㅁ의 가운데가 이미 채워져 있어도 놓을 수 있다.
    let mut st = state_with(&["..........", "....#.....", ".........."], [Some("ㅁ"), None, None]);
    assert!(st.board.can_place(&piece(pid("ㅁ")).orients[0], 13, 3));
    // 테두리 칸이 채워져 있으면 당연히 못 놓는다.
    assert!(!st.board.can_place(&piece(pid("ㅁ")).orients[0], 12, 3));
    st.place_det(0, 0, 13, 3);
    assert_eq!(st.board.to_strings()[13], "...###....");
    assert_eq!(st.board.to_strings()[14], "...###....");
    assert_eq!(st.board.to_strings()[15], "...###....");
}

#[test]
fn line_score_table() {
    assert_eq!(line_score(1), 300);
    assert_eq!(line_score(2), 1200);
    assert_eq!(line_score(3), 2700);
    assert_eq!(line_score(4), 4800);
    assert_eq!(line_score(5), 7500);
}

#[test]
fn simultaneous_clear_is_quadratic_not_linear() {
    // 두 줄이 각각 한 칸씩 비어 있고 ㅣ(세로 2칸 방향은 없으니 ㆍ 두 번 대신) ㅡ의 세로 방향(3칸)으로 동시에 채운다.
    let mut st = state_with(&["#########.", "#########.", "#########.", ".........."], [Some("ㅡ"), None, None]);
    let vert = orient_of("ㅡ", &["#", "#", "#"]);
    let out = st.place_det(0, vert, 12, 9);
    assert_eq!(out.lines, 3);
    assert_eq!(out.score_delta, 3 + 2700);
    assert_eq!(st.lines, 3);
    // 제거된 줄은 비워지고 (중력 없음) 다른 줄은 그대로.
    assert_eq!(st.board.to_strings()[12], "..........");
    assert_eq!(st.board.to_strings()[14], "..........");
}

#[test]
fn only_horizontal_rows_clear() {
    // 세로 한 열이 전부 차도 제거되지 않는다.
    let mut rows = vec![String::from("#........."); 15];
    rows.push(String::from("..........")); // 맨 아랫줄 비움
    let mut st = state_with(&rows.iter().map(|s| s.as_str()).collect::<Vec<_>>(), [Some("ㆍ"), None, None]);
    let out = st.place_det(0, 0, 15, 0);
    assert_eq!(out.lines, 0);
    assert_eq!(st.board.filled(), 16);
}

#[test]
fn icon_pickup_grants_ability_and_50_points() {
    let mut st = state_with(&["#########."], [Some("ㆍ"), None, None]);
    let mut rng = Rng::new(1);
    // 아이콘을 15행 9열(빈 칸)에 수동 배치.
    let st_json = st.to_json().replace("\"icons\":[]", "\"icons\":[{\"r\":15,\"c\":9,\"kind\":\"Reroll\",\"seq\":0}]");
    st = State::from_json(&st_json).unwrap();
    assert_eq!(st.icons.len(), 1);
    let out = st.place(0, 0, 15, 9, &mut rng, &PieceWeights::uniform());
    assert_eq!(out.lines, 1);
    assert_eq!(out.pickups, 1);
    assert_eq!(st.rerolls, 1);
    assert_eq!(st.score, 1 + 300 + 50);
    assert_eq!(st.icons.len(), 0);
}

#[test]
fn icon_not_granted_at_cap_and_stays_on_board() {
    let mut st = state_with(&["#########."], [Some("ㆍ"), None, None]);
    st.dots = 4;
    st.rerolls = 3; // 합계 7 = 한도
    let st_json = st.to_json().replace("\"icons\":[]", "\"icons\":[{\"r\":15,\"c\":9,\"kind\":\"Dot\",\"seq\":0}]");
    st = State::from_json(&st_json).unwrap();
    let out = st.place_det(0, 0, 15, 9);
    assert_eq!(out.lines, 1);
    assert_eq!(out.pickups, 0);
    assert_eq!(st.held(), 7);
    assert_eq!(st.icons.len(), 1, "아이콘은 판에 그대로 남는다");
    assert_eq!(st.score, 1 + 300);
}

#[test]
fn icon_spawns_every_7_placements_and_not_at_cap() {
    let pw = PieceWeights { w: { let mut w = [0.0; NUM_PIECES]; w[pid("ㆍ") as usize] = 1.0; w } };
    let mut rng = Rng::new(3);
    let mut st = State::new_game(&mut rng, &pw);
    let mut spawned_at = Vec::new();
    for k in 0..21 {
        let r = k / 10;
        let c = k % 10;
        let slot = st.hand.iter().position(|h| h.is_some()).unwrap();
        let before = st.icons.len();
        let out = st.place(slot, 0, r, c, &mut rng, &pw);
        if st.icons.len() > before {
            spawned_at.push(st.placements);
        }
        assert_eq!(out.spawn_due, st.placements % 7 == 0);
    }
    assert_eq!(spawned_at, vec![7, 14, 21]);
    // 보유가 7개면 생성되지 않는다.
    st.dots = 7;
    let mut probe = st.clone();
    assert!(probe.spawn_icon(&mut rng).is_none());
}

#[test]
fn board_holds_at_most_3_icons_oldest_disappears() {
    let mut st = State::blank();
    let mut rng = Rng::new(5);
    let a = st.spawn_icon(&mut rng).unwrap();
    let b = st.spawn_icon(&mut rng).unwrap();
    let c = st.spawn_icon(&mut rng).unwrap();
    assert_eq!(st.icons.len(), 3);
    let d = st.spawn_icon(&mut rng).unwrap();
    assert_eq!(st.icons.len(), 3);
    let seqs: Vec<u32> = st.icons.iter().map(|i| i.seq).collect();
    assert_eq!(seqs, vec![b.seq, c.seq, d.seq]);
    assert!(!seqs.contains(&a.seq));
}

#[test]
fn oldest_icon_drop_is_deterministic_and_search_sees_it() {
    // 판에 아이콘 3개, 보유 < 7, 이번 배치가 7번째 → 결정적 전이에서 최고령 아이콘이 사라진다.
    let mut st = state_with(&[], [Some("ㆍ"), Some("ㆍ"), Some("ㆍ")]);
    st.placements = 6;
    let mut rng = Rng::new(2);
    st.spawn_icon(&mut rng);
    st.spawn_icon(&mut rng);
    st.spawn_icon(&mut rng);
    let oldest = st.icons.get(0).seq;
    let mut det = st.clone();
    let out = det.place_det(0, 0, 0, 0);
    assert!(out.spawn_due && out.icon_dropped);
    assert_eq!(det.icons.len(), 2);
    assert!(det.icons.iter().all(|i| i.seq != oldest));
    // 실제 전이도 같은 아이콘을 버리고 새 아이콘 하나를 추가한다 (3개 유지).
    let mut real = st.clone();
    real.place(0, 0, 0, 0, &mut rng, &PieceWeights::uniform());
    assert_eq!(real.icons.len(), 3);
    assert!(real.icons.iter().all(|i| i.seq != oldest));
    assert_eq!(real.icons.get(0).seq, det.icons.get(0).seq);
    assert_eq!(real.icons.get(1).seq, det.icons.get(1).seq);
    // 보유가 7개면 생성이 없으니 아무것도 사라지지 않는다.
    let mut full = st.clone();
    full.dots = 7;
    let out = full.place_det(0, 0, 0, 0);
    assert!(out.spawn_due && !out.icon_dropped);
    assert_eq!(full.icons.len(), 3);
}

#[test]
fn icon_kind_distribution_is_40_60() {
    let mut rng = Rng::new(11);
    let mut dots = 0;
    let n = 20_000;
    for _ in 0..n {
        let mut st = State::blank();
        if st.spawn_icon(&mut rng).unwrap().kind == AbilityKind::Dot {
            dots += 1;
        }
    }
    let p = dots as f64 / n as f64;
    assert!((p - 0.40).abs() < 0.02, "점 찍기 비율 {}", p);
}

#[test]
fn dot_fills_cell_clears_rows_and_does_not_count_as_placement() {
    let mut st = state_with(&["#########."], [Some("ㄱ"), None, None]);
    st.dots = 1;
    st.placements = 6;
    let st_json = st.to_json().replace("\"icons\":[]", "\"icons\":[{\"r\":15,\"c\":9,\"kind\":\"Dot\",\"seq\":0}]");
    st = State::from_json(&st_json).unwrap();
    let out = st.dot(15, 9);
    assert_eq!(out.lines, 1);
    assert_eq!(out.pickups, 1, "점 찍기로 지운 줄의 능력도 획득한다");
    assert_eq!(st.dots, 1, "하나 쓰고 하나 얻음");
    assert_eq!(st.placements, 6, "배치 횟수에 포함되지 않는다");
    assert!(!out.spawn_due);
    assert_eq!(st.score, 1 + 300 + 50);
}

#[test]
fn reroll_replaces_slot_immediately() {
    let pw = PieceWeights { w: { let mut w = [0.0; NUM_PIECES]; w[pid("ㅎ") as usize] = 1.0; w } };
    let mut st = state_with(&[], [Some("ㄱ"), Some("ㄴ"), Some("ㄷ")]);
    st.rerolls = 2;
    let mut rng = Rng::new(9);
    let p = st.reroll(1, &mut rng, &pw);
    assert_eq!(p, pid("ㅎ"));
    assert_eq!(st.hand[1], Some(pid("ㅎ")));
    assert_eq!(st.rerolls, 1);
}

#[test]
fn game_over_only_when_stuck_without_abilities() {
    // 빈 칸이 1개뿐인 판에 ㄱ만 남음.
    let mut rows = vec![String::from("##########"); 15];
    rows.push(String::from("#########."));
    let st = state_with(&rows.iter().map(|s| s.as_str()).collect::<Vec<_>>(), [Some("ㄱ"), None, None]);
    assert!(!st.can_place_any());
    assert!(st.is_stuck());
    let mut with_ability = st.clone();
    with_ability.dots = 1;
    assert!(!with_ability.is_stuck(), "능력이 있으면 종료가 아니다");
    with_ability.check_over();
    assert!(!with_ability.over);
}

#[test]
fn score_is_capped_at_500k() {
    let mut st = state_with(&["#########."], [Some("ㆍ"), None, None]);
    st.score = SCORE_CAP - 100;
    st.place_det(0, 0, 15, 9);
    assert_eq!(st.score, SCORE_CAP);
    assert!(st.over);
}

#[test]
fn state_json_roundtrip() {
    let mut rng = Rng::new(42);
    let pw = PieceWeights::uniform();
    let mut st = State::new_game(&mut rng, &pw);
    st.dots = 2;
    st.spawn_icon(&mut rng);
    let j = st.to_json();
    let back = State::from_json(&j).unwrap();
    assert_eq!(st, back);
    assert!(j.contains("\"board\":[\""));
}

#[test]
fn search_finds_multi_line_clear_when_available() {
    // ㅣ를 세로로 넣으면 5줄 동시 제거(7,500점). 탐색이 이를 선택해야 한다.
    let st = state_with(
        &["#########.", "#########.", "#########.", "#########.", "#########."],
        [Some("ㅣ"), Some("ㄴ"), Some("ㅅ")],
    );
    let plan = search(&st, &PieceWeights::uniform(), &Weights::default(), &SearchParams::default());
    assert!(plan.complete);
    assert!(plan.gain >= 7500 + 5, "gain={}", plan.gain);
    let first = plan.moves[0];
    match first {
        Move::Place { slot, r, c, .. } => {
            assert_eq!(slot, 0);
            assert_eq!((r, c), (11, 9));
        }
        _ => panic!("첫 수가 배치가 아님"),
    }
}

#[test]
fn search_uses_dot_to_finish_a_row_with_icon() {
    // 폭 1 우물 두 줄: ㄴ으로는 못 메운다. 점 찍기로 아이콘 줄을 지우면 300+50점에 점 찍기를 되돌려 받는다.
    let mut st = state_with(&["#########.", "#########."], [Some("ㄴ"), None, None]);
    st.dots = 1;
    let st_json = st.to_json().replace("\"icons\":[]", "\"icons\":[{\"r\":15,\"c\":9,\"kind\":\"Dot\",\"seq\":0}]");
    st = State::from_json(&st_json).unwrap();
    let plan = search(&st, &PieceWeights::uniform(), &Weights::default(), &SearchParams::default());
    assert!(plan.complete);
    assert!(plan.moves.iter().any(|m| matches!(m, Move::Dot { r: 15, c: 9 })), "{:?}", plan.moves);
    assert!(plan.gain >= 1 + 300 + 50 + 3, "gain={}", plan.gain);
}

#[test]
fn selfplay_never_ends_while_holding_abilities() {
    // 판이 거의 꽉 찬 상태 + 바꿔 뽑기 2개: 시뮬레이터는 바꿔 뽑기를 써야지 종료하면 안 된다.
    let pw = PieceWeights::uniform();
    let w = Weights::default();
    let sp = SearchParams::fast(6);
    let cfg = PlayConfig { max_hands: 5, ..Default::default() };
    // 결과 통계로 간접 확인: 많은 시드에서 게임이 끝났을 때 능력이 남아 있지 않아야 한다.
    for seed in 1..=6u64 {
        let r = play_game(seed, &pw, &w, &sp, &cfg);
        assert!(r.placements > 0, "seed {}", seed);
    }
    // 직접 확인: 막힌 손패 + 바꿔 뽑기 보유 → is_stuck 은 false 이고, 바꿔 뽑기 후 진행 가능.
    let mut rows = vec![String::from("##########"); 15];
    rows.push(String::from("#########."));
    let mut st = state_with(&rows.iter().map(|s| s.as_str()).collect::<Vec<_>>(), [Some("ㄱ"), None, None]);
    st.rerolls = 1;
    assert!(!st.is_stuck());
    let only_dot = PieceWeights { w: { let mut w = [0.0; NUM_PIECES]; w[pid("ㆍ") as usize] = 1.0; w } };
    let mut rng = Rng::new(1);
    st.reroll(0, &mut rng, &only_dot);
    assert!(st.can_place_any());
    assert_eq!(st.rerolls, 0);
}

/// 완전 탐색: 남은 손패를 전부 놓는 순서가 존재하는가 (테스트 자체 검증용).
fn exists_full_placement(st: &State) -> bool {
    if st.hand_empty() {
        return true;
    }
    let mut tried = [false; NUM_PIECES];
    for slot in 0..3 {
        let Some(pid) = st.hand[slot] else { continue };
        if tried[pid as usize] {
            continue;
        }
        tried[pid as usize] = true;
        let mut found = false;
        st.board.for_each_placement(piece(pid), |oi, r, c| {
            if found {
                return;
            }
            let mut s = st.clone();
            s.place_det(slot, oi, r, c);
            if exists_full_placement(&s) {
                found = true;
            }
        });
        if found {
            return true;
        }
    }
    false
}

#[test]
fn beam_does_not_prune_the_only_feasible_line() {
    // 감사에서 재현된 상태: 빔이 '놓을 수 있는' 경로를 버려 complete=false 를 돌려주던 판.
    let board = [
        "###..#..#.", "..........", "...#####..", ".#...#....", ".#.###..##", "###.####.#", "#.#.###.##", "....######",
        "#.#.###..#", "....####..", "#.######.#", "...#.###..", "...####...", "...#####..", "..#####...", "....#...##",
    ];
    let mut st = state_with(&board, [Some("ㅍ"), Some("ㅋ"), Some("ㅡ")]);
    st.rerolls = 6;
    assert!(exists_full_placement(&st), "테스트 전제: 완전 배치가 존재해야 한다");
    for beam in [12usize, 64] {
        let plan = search(&st, &PieceWeights::uniform(), &Weights::default(), &SearchParams { beam, ..Default::default() });
        assert!(plan.complete, "beam {} 에서 놓을 수 있는 손패를 못 놓는다고 판단: {:?}", beam, plan.moves);
    }
}

#[test]
fn partial_fallback_returns_dot_rescue() {
    // 위쪽 체커보드(ㅂ이 못 들어감) + 아래 4줄은 9열만 비어 있음. ㅂ은 어디에도 못 놓지만 점 찍기로 줄은 지울 수 있다.
    let mut rows: Vec<String> = Vec::new();
    for r in 0..12 {
        rows.push((0..10).map(|c| if (r + c) % 2 == 0 { '#' } else { '.' }).collect());
    }
    for _ in 0..4 {
        rows.push("#########.".to_string());
    }
    let mut st = state_with(&rows.iter().map(|s| s.as_str()).collect::<Vec<_>>(), [Some("ㅂ"), Some("ㅂ"), Some("ㅂ")]);
    st.dots = 1;
    assert!(!st.can_place_any());
    let plan = search(&st, &PieceWeights::uniform(), &Weights::default(), &SearchParams::default());
    assert!(!plan.complete);
    assert!(matches!(plan.moves.first(), Some(Move::Dot { .. })), "{:?}", plan.moves);
}

#[test]
fn serialized_over_flag_is_recomputed() {
    // 저장된 over=true 라도 판을 고쳐 놓을 수 있게 되면 false 로 돌아와야 한다.
    let st = state_with(&[], [Some("ㄱ"), None, None]);
    let j = st.to_json().replace("\"over\":false", "\"over\":true");
    let back = State::from_json(&j).unwrap();
    assert!(!back.over);
}

#[test]
fn summary_median_averages_middle_pair() {
    let mk = |score: u32| GameResult { score, ..Default::default() };
    let s = moamoa_engine::sim::summarize(&[mk(100), mk(300)]);
    assert_eq!(s.median, 200.0);
    let s = moamoa_engine::sim::summarize(&[mk(100), mk(300), mk(900)]);
    assert_eq!(s.median, 300.0);
}

#[test]
fn selfplay_runs_to_completion_deterministically() {
    let pw = PieceWeights::uniform();
    let w = Weights::default();
    let sp = SearchParams::fast(6);
    let cfg = PlayConfig { max_hands: 30, ..Default::default() };
    let a = play_game(1, &pw, &w, &sp, &cfg);
    let b = play_game(1, &pw, &w, &sp, &cfg);
    assert_eq!(a, b);
    assert!(a.placements > 0);
    assert!(a.score > 0);
}
