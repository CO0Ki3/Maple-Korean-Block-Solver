//! moamoa CLI: selfplay / bench / train / pieces
//!
//! 예)
//!   moamoa selfplay --games 20 --beam 32 --threads 12
//!   moamoa train --pop 40 --elite 8 --gens 40 --games 6 --beam 12 --max-hands 150 --out weights/runs/r1
//!   moamoa bench

use moamoa_engine::sim::{play_game, policy_step, summarize, Event, GameResult, PlayConfig, Summary};
use moamoa_engine::{pieces, value_full, NTuple, PieceWeights, Rng, SearchParams, State, Weights, FEATURE_NAMES, NF};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

struct Args {
    cmd: String,
    kv: HashMap<String, String>,
}

fn parse_args() -> Args {
    let mut it = std::env::args().skip(1);
    let cmd = it.next().unwrap_or_else(|| "help".into());
    let mut kv = HashMap::new();
    let mut key: Option<String> = None;
    for a in it {
        if let Some(k) = a.strip_prefix("--") {
            if let Some(prev) = key.take() {
                kv.insert(prev, "true".into());
            }
            key = Some(k.to_string());
        } else if let Some(k) = key.take() {
            kv.insert(k, a);
        }
    }
    if let Some(k) = key {
        kv.insert(k, "true".into());
    }
    Args { cmd, kv }
}

impl Args {
    fn get<T: std::str::FromStr>(&self, k: &str, default: T) -> T {
        self.kv.get(k).and_then(|v| v.parse().ok()).unwrap_or(default)
    }
    fn flag(&self, k: &str) -> bool {
        self.kv.get(k).map(|v| v == "true" || v == "1").unwrap_or(false)
    }
    fn str(&self, k: &str) -> Option<&str> {
        self.kv.get(k).map(|s| s.as_str())
    }
}

fn load_weights(a: &Args) -> Weights {
    let mut w = match a.str("weights") {
        Some(p) => {
            let s = std::fs::read_to_string(p).unwrap_or_else(|e| panic!("가중치 파일을 읽지 못했습니다 {}: {}", p, e));
            Weights::from_json(&s).unwrap_or_else(|e| panic!("가중치 파싱 실패: {}", e))
        }
        None => Weights::default(),
    };
    if let Some(p) = a.str("ntuple") {
        let s = std::fs::read_to_string(p).unwrap_or_else(|e| panic!("N-tuple 파일을 읽지 못했습니다 {}: {}", p, e));
        w.ntuple = Some(Arc::new(NTuple::from_json(&s).unwrap_or_else(|e| panic!("N-tuple 파싱 실패: {}", e))));
    }
    w
}

fn load_pw(a: &Args) -> PieceWeights {
    match a.str("pw") {
        Some(p) => {
            let s = std::fs::read_to_string(p).unwrap_or_else(|e| panic!("조각 가중치 파일을 읽지 못했습니다 {}: {}", p, e));
            serde_json::from_str(&s).unwrap_or_else(|e| panic!("조각 가중치 파싱 실패: {}", e))
        }
        None => PieceWeights::uniform(),
    }
}

fn search_params(a: &Args) -> SearchParams {
    SearchParams {
        beam: a.get("beam", 32usize),
        max_dots: a.get("max-dots", 2u8),
        leaf_k: a.get("leaf-k", 32usize),
        samples: a.get("samples", 0usize),
        samples_top: a.get("samples-top", 0usize),
        sample_beam: a.get("sample-beam", 12usize),
        line_bonus: a.get("line-bonus", 0.0f32),
        alts: 3,
        ..Default::default()
    }
}

fn play_config(a: &Args) -> PlayConfig {
    PlayConfig {
        max_hands: a.get("max-hands", 0u32),
        replan_each_move: a.flag("replan"),
        reroll_margin: a.get("reroll-margin", 0.0f32),
        reroll_at_cap: !a.flag("no-reroll-at-cap"),
        reroll_beam: a.get("reroll-beam", 8usize),
        replan_on_spawn: !a.flag("no-replan-on-spawn"),
        reroll_always: a.flag("reroll-always"),
    }
}

/// 작업 목록을 스레드 풀로 돌린다. 결과는 입력 순서대로.
fn parallel_map<T: Send + Sync, R: Send>(items: &[T], threads: usize, f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let next = AtomicUsize::new(0);
    let out: Mutex<Vec<Option<R>>> = Mutex::new((0..items.len()).map(|_| None).collect());
    std::thread::scope(|s| {
        for _ in 0..threads.max(1) {
            s.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                if i >= items.len() {
                    break;
                }
                let r = f(&items[i]);
                out.lock().unwrap()[i] = Some(r);
            });
        }
    });
    out.into_inner().unwrap().into_iter().map(|x| x.unwrap()).collect()
}

fn print_summary(s: &Summary, secs: f64) {
    println!(
        "games={} mean={:.0} median={:.0} min={} max={} lines={:.1} hands={:.1} multi={:.1}% clears[1..5]={:?} capped={} time={:.1}s",
        s.games, s.mean, s.median, s.min, s.max, s.mean_lines, s.mean_hands, s.multi_ratio * 100.0, &s.clears[1..], s.capped, secs
    );
}

fn cmd_selfplay(a: &Args) {
    let games = a.get("games", 10u64);
    let seed0 = a.get("seed", 1u64);
    let threads = a.get("threads", 12usize);
    let w = load_weights(a);
    let pw = load_pw(a);
    let sp = search_params(a);
    let cfg = play_config(a);
    let verbose = a.flag("verbose");
    let t0 = Instant::now();
    let seeds: Vec<u64> = (0..games).map(|i| seed0 + i).collect();
    let results: Vec<GameResult> = parallel_map(&seeds, threads, |&seed| {
        let r = play_game(seed, &pw, &w, &sp, &cfg);
        if verbose {
            println!(
                "seed={} score={} lines={} hands={} clears={:?} pickups={} dots={} rerolls={} capped={} trunc={}",
                r.seed, r.score, r.lines, r.hands, &r.clears[1..], r.pickups, r.dots_used, r.rerolls_used, r.capped, r.truncated
            );
        }
        r
    });
    let s = summarize(&results);
    print_summary(&s, t0.elapsed().as_secs_f64());
}

fn cmd_bench(a: &Args) {
    let w = load_weights(a);
    let pw = PieceWeights::uniform();
    let sp = search_params(a);
    // 중반 상태를 만들기 위해 몇 손패를 둔다.
    let mut rng = Rng::new(7);
    let mut st = State::new_game(&mut rng, &pw);
    let warm = SearchParams::fast(8);
    for _ in 0..12 {
        let plan = moamoa_engine::search(&st, &pw, &w, &warm);
        if !plan.complete {
            break;
        }
        for mv in plan.moves {
            if let moamoa_engine::Move::Place { slot, orient, r, c } = mv {
                st.place(slot as usize, orient as usize, r as usize, c as usize, &mut rng, &pw);
            }
        }
    }
    println!("{}", st.board);
    println!("hand={:?} filled={}", st.hand, st.board.filled());
    let t0 = Instant::now();
    let n = 10;
    let mut nodes = 0u64;
    for _ in 0..n {
        let plan = moamoa_engine::search(&st, &pw, &w, &sp);
        nodes += plan.nodes as u64;
    }
    let dt = t0.elapsed().as_secs_f64();
    println!(
        "beam={} samples={}: {:.1} ms/search, {:.2} M nodes/s",
        sp.beam,
        sp.samples,
        dt * 1000.0 / n as f64,
        nodes as f64 / dt / 1e6
    );
}

fn cmd_pieces() {
    for p in pieces() {
        println!("{} {} n={} orients={}", p.id, p.name, p.n, p.orients.len());
        for (i, o) in p.orients.iter().enumerate() {
            let mut s = String::new();
            for r in 0..o.h as usize {
                for c in 0..o.w as usize {
                    s.push(if (o.rows[r] >> c) & 1 == 1 { '#' } else { '.' });
                }
                s.push('/');
            }
            println!("  [{}] {}×{} {}", i, o.h, o.w, s);
        }
    }
}

/// 교차 엔트로피 방법으로 가중치를 학습한다.
fn cmd_train(a: &Args) {
    let pop = a.get("pop", 40usize);
    let elite = a.get("elite", 8usize);
    let gens = a.get("gens", 30usize);
    let games = a.get("games", 6u64);
    let threads = a.get("threads", 12usize);
    let seed0 = a.get("seed", 1000u64);
    let out = a.str("out").unwrap_or("weights/runs/latest").to_string();
    let noise0 = a.get("noise", 1.0f64);
    let sigma_scale = a.get("sigma", 0.3f64);
    let val_games = a.get("val-games", 24u64);
    // 고정 검증 시드: 세대마다 평가 시드가 달라 fitness를 세대 간에 비교할 수 없으므로 best.json은 이 집합으로 고른다.
    let val_seed = seed0 + 100_000_000;
    assert!(pop >= 2, "--pop 은 2 이상이어야 합니다");
    let elite = elite.clamp(1, pop);
    std::fs::create_dir_all(&out).expect("출력 폴더");
    let pw = load_pw(a);
    let sp = search_params(a);
    let cfg = play_config(a);
    let mut mu = load_weights(a);
    let mut sigma = [0f64; NF];
    for i in 0..NF {
        sigma[i] = (mu.w[i].abs() as f64) * sigma_scale + 2.0;
    }
    let mut rng = Rng::new(seed0 ^ 0xC0FFEE);
    let mut best_ever: (f64, Weights) = (f64::NEG_INFINITY, mu.clone());
    println!("train: pop={} elite={} gens={} games/cand={} beam={} max_hands={} threads={} out={}", pop, elite, gens, games, sp.beam, cfg.max_hands, threads, out);

    for g in 0..gens {
        let t0 = Instant::now();
        // 후보 생성 (0번은 현재 평균 자체).
        let mut cands: Vec<Weights> = Vec::with_capacity(pop);
        cands.push(mu.clone());
        for _ in 1..pop {
            let mut w = mu.clone();
            for i in 0..NF {
                w.w[i] = (mu.w[i] as f64 + sigma[i] * rng.normal()) as f32;
            }
            cands.push(w);
        }
        // 공통 난수: 이 세대의 모든 후보가 같은 시드 집합으로 평가된다.
        let gen_seed = seed0 + (g as u64) * 10_000;
        let tasks: Vec<(usize, u64)> = (0..pop).flat_map(|ci| (0..games).map(move |k| (ci, gen_seed + k))).collect();
        let scores = parallel_map(&tasks, threads, |&(ci, seed)| {
            let r = play_game(seed, &pw, &cands[ci], &sp, &cfg);
            r.score as f64
        });
        let mut fitness = vec![0f64; pop];
        for (t, &(ci, _)) in tasks.iter().enumerate() {
            fitness[ci] += scores[t] / games as f64;
        }
        let mut order: Vec<usize> = (0..pop).collect();
        order.sort_by(|&x, &y| fitness[y].partial_cmp(&fitness[x]).unwrap());
        let elites: Vec<usize> = order.iter().copied().take(elite).collect();
        // 평균·표준편차 갱신 + 잡음 바닥 (세대가 갈수록 줄어든다).
        let noise = noise0 * (1.0 - g as f64 / gens as f64).max(0.0);
        let mut new_mu = [0f64; NF];
        for &e in &elites {
            for i in 0..NF {
                new_mu[i] += cands[e].w[i] as f64 / elite as f64;
            }
        }
        for i in 0..NF {
            let mut var = 0f64;
            for &e in &elites {
                let d = cands[e].w[i] as f64 - new_mu[i];
                var += d * d / elite as f64;
            }
            sigma[i] = (var.sqrt() + noise).max(1e-3);
            mu.w[i] = new_mu[i] as f32;
        }
        let best_i = order[0];
        let val = {
            let seeds: Vec<u64> = (0..val_games).map(|k| val_seed + k).collect();
            let sc = parallel_map(&seeds, threads, |&s| play_game(s, &pw, &cands[best_i], &sp, &cfg).score as f64);
            sc.iter().sum::<f64>() / val_games.max(1) as f64
        };
        if val > best_ever.0 {
            best_ever = (val, cands[best_i].clone());
            std::fs::write(format!("{}/best.json", out), best_ever.1.to_json()).unwrap();
        }
        std::fs::write(format!("{}/gen_{:03}.json", out, g), mu.to_json()).unwrap();
        std::fs::write(format!("{}/latest.json", out), mu.to_json()).unwrap();
        let mean_fit: f64 = fitness.iter().sum::<f64>() / pop as f64;
        let elite_mean: f64 = elites.iter().map(|&e| fitness[e]).sum::<f64>() / elite as f64;
        println!(
            "gen {:3} | mu(prev)={:.0} best={:.0} (val {:.0}) elite_mean={:.0} pop_mean={:.0} best_val_ever={:.0} | {:.0}s",
            g,
            fitness[0],
            fitness[best_i],
            val,
            elite_mean,
            mean_fit,
            best_ever.0,
            t0.elapsed().as_secs_f64()
        );
        let line: Vec<String> = FEATURE_NAMES.iter().zip(mu.w.iter()).map(|(n, v)| format!("{}={:.1}", n, v)).collect();
        println!("      mu: {}", line.join(" "));
    }
    println!("done. best_ever={:.0} → {}/best.json, final mean → {}/latest.json", best_ever.0, out, out);
}

/// 한 스레드의 자기대전 학습: 손패 경계마다 afterstate를 기록하고, `horizon` 손패 뒤의 잘린 할인 수익
/// G = Σ γ^k r_k + γ^N V(s_N) (게임이 끝나면 부트스트랩 없이 0)을 목표로 N-tuple 잔차를 갱신한다.
/// (손패 수, 끝난 판 수, 점수 합)
#[allow(clippy::too_many_arguments)]
fn td_worker(seed: u64, hands: u64, act: &Weights, w: &Weights, pw: &PieceWeights, sp: &SearchParams, cfg: &PlayConfig, alpha: f32, alpha_bias: f32, gamma: f32, clip: f32, horizon: usize) -> (u64, u64, u64) {
    use std::collections::VecDeque;
    let nt = w.ntuple.as_ref().expect("ntuple");
    let scale = alpha / nt.active as f32;
    let mut rng = Rng::new(seed);
    let mut st = State::new_game(&mut rng, pw);
    // (afterstate, 그 시점 점수)
    let mut buf: VecDeque<(State, u32)> = VecDeque::new();
    let (mut done, mut games, mut score_sum) = (0u64, 0u64, 0u64);
    let mut guard = 0u32;
    let apply = |s0: &State, target: f32| {
        let d = (target - value_full(s0, pw, w)).clamp(-clip, clip);
        nt.update(&s0.board, scale * d);
        nt.add_bias(alpha_bias * d);
    };
    while done < hands {
        if st.over {
            // 종료: 버퍼의 모든 상태를 '끝까지의 할인 보상'으로 갱신 (부트스트랩 없음).
            let n = buf.len();
            for i in 0..n {
                let mut g = 0f32;
                let mut disc = 1f32;
                for k in i..n {
                    let next_score = if k + 1 < n { buf[k + 1].1 } else { st.score };
                    g += disc * (next_score as f32 - buf[k].1 as f32);
                    disc *= gamma;
                }
                apply(&buf[i].0, g);
            }
            buf.clear();
            games += 1;
            score_sum += st.score as u64;
            st = State::new_game(&mut rng, pw);
            guard = 0;
            continue;
        }
        guard += 1;
        if guard > 50_000 {
            st = State::new_game(&mut rng, pw);
            buf.clear();
            guard = 0;
            continue;
        }
        let events = policy_step(&mut st, &mut rng, pw, act, sp, cfg);
        if events.is_empty() {
            st.over = true;
            continue;
        }
        let hand_done = events.iter().any(|e| matches!(e, Event::Place { new_hand: Some(_), .. }));
        if hand_done {
            done += 1;
            buf.push_back((st.clone(), st.score));
            if buf.len() > horizon {
                // 가장 오래된 상태의 목표: N손패 동안의 할인 보상 + γ^N V(N번째 상태)
                let mut g = 0f32;
                let mut disc = 1f32;
                for k in 0..horizon {
                    g += disc * (buf[k + 1].1 as f32 - buf[k].1 as f32);
                    disc *= gamma;
                }
                g += disc * value_full(&buf[horizon].0, pw, w);
                apply(&buf[0].0, g);
                buf.pop_front();
            }
        }
    }
    (done, games, score_sum)
}

/// 증류 학습 워커: 행동 정책은 '목표 평가(tgt) + 샘플링 lookahead' 탐색이며, 그 탐색이 평가한 상위 리프들
/// (리프 상태, lookahead 값)을 표본으로 혼합 V(선형 + N-tuple)를 회귀시킨다. (손패 수, 판 수, 점수 합, 표본 수)
#[allow(clippy::too_many_arguments)]
fn distill_worker(seed: u64, hands: u64, tgt: &Weights, w: &Weights, pw: &PieceWeights, sp: &SearchParams, cfg: &PlayConfig, alpha: f32, alpha_bias: f32, clip: f32) -> (u64, u64, u64, u64, f64, f64) {
    let nt = w.ntuple.as_ref().expect("ntuple");
    let scale = alpha / nt.active as f32;
    let mut rng = Rng::new(seed);
    let mut st = State::new_game(&mut rng, pw);
    let (mut done, mut games, mut score_sum, mut samples) = (0u64, 0u64, 0u64, 0u64);
    // 회귀 오차 (갱신 전 기준): 혼합 V 와 선형 V 의 제곱 오차 합
    let (mut se_mix, mut se_lin) = (0f64, 0f64);
    let mut guard = 0u32;
    let mut leaves: Vec<(State, f32)> = Vec::new();
    while done < hands {
        if st.over {
            games += 1;
            score_sum += st.score as u64;
            st = State::new_game(&mut rng, pw);
            guard = 0;
            continue;
        }
        guard += 1;
        if guard > 50_000 {
            st = State::new_game(&mut rng, pw);
            guard = 0;
            continue;
        }
        leaves.clear();
        let events = moamoa_engine::policy_step_collect(&mut st, &mut rng, pw, tgt, sp, cfg, Some(&mut leaves));
        if events.is_empty() {
            st.over = true;
            continue;
        }
        for (leaf, target) in leaves.iter() {
            let vm = value_full(leaf, pw, w);
            let vl = value_full(leaf, pw, tgt);
            se_mix += ((target - vm) as f64).powi(2);
            se_lin += ((target - vl) as f64).powi(2);
            let d = (target - vm).clamp(-clip, clip);
            nt.update(&leaf.board, scale * d);
            nt.add_bias(alpha_bias * d);
            samples += 1;
        }
        if events.iter().any(|e| matches!(e, Event::Place { new_hand: Some(_), .. })) {
            done += 1;
        }
    }
    (done, games, score_sum, samples, se_mix, se_lin)
}

/// 깊은 탐색 값을 N-tuple 잔차에 증류한다 (NNUE 방식). 행동·목표 탐색: 선형 평가 + 샘플링.
fn cmd_train_distill(a: &Args) {
    let threads = a.get("threads", 11usize);
    let epochs = a.get("epochs", 6usize);
    let hands = a.get("hands", 300u64);
    let alpha = a.get("alpha", 0.05f32);
    let alpha_bias = a.get("alpha-bias", 0.01f32);
    let clip = a.get("clip", 3000.0f32);
    let seed0 = a.get("seed", 8000u64);
    let eval_games = a.get("eval-games", 24u64);
    let eval_hands = a.get("eval-hands", 200u32);
    let out = a.str("out").unwrap_or("weights/runs/distill1").to_string();
    std::fs::create_dir_all(&out).expect("출력 폴더");
    let pw = load_pw(a);
    let base = load_weights(a);
    let nt: Arc<NTuple> = match &base.ntuple {
        Some(n) => n.clone(),
        None => Arc::new(NTuple::with_mode(NTuple::default_shapes(), a.flag("per-position"))),
    };
    let w = Weights { w: base.w, ntuple: Some(nt.clone()) };
    let lin = Weights { w: base.w, ntuple: None };
    // 행동·목표 탐색: --beam, --samples(기본 4), --sample-beam, --leaf-k(기본 12)
    let sp = SearchParams { samples: a.get("samples", 4usize).max(1), leaf_k: a.get("leaf-k", 12usize), ..search_params(a) };
    let proxy_sp = SearchParams { samples: 0, samples_top: 0, leaf_k: 32, ..sp.clone() };
    let cfg = play_config(a);
    println!("train-distill: shapes={:?} per_position={} entries={} active={} threads={} epochs={} hands/thread={} alpha={} alpha_bias={} act/target: beam={} samples={} sample_beam={} leaf_k={} out={}", nt.shapes, nt.per_position, nt.len(), nt.active, threads, epochs, hands, alpha, alpha_bias, sp.beam, sp.samples, sp.sample_beam, sp.leaf_k, out);
    let t0 = Instant::now();
    let base_proxy = proxy_eval(&lin, &pw, &proxy_sp, eval_hands, eval_games, 9_000_000, threads);
    println!("proxy({}판×{}손패, 빔 {}, 샘플 0): 선형={:.0} [{:.0}s]", eval_games, eval_hands, proxy_sp.beam, base_proxy, t0.elapsed().as_secs_f64());
    let mut best = (f64::NEG_INFINITY, 0usize);
    for ep in 0..epochs {
        let te = Instant::now();
        let seeds: Vec<u64> = (0..threads as u64).map(|t| seed0 + (ep as u64) * 1000 + t).collect();
        let res = parallel_map(&seeds, threads, |&s| distill_worker(s, hands, &lin, &w, &pw, &sp, &cfg, alpha, alpha_bias, clip));
        let (h, g, sc, ns, se_m, se_l) = res.iter().fold((0u64, 0u64, 0u64, 0u64, 0f64, 0f64), |acc, r| (acc.0 + r.0, acc.1 + r.1, acc.2 + r.2, acc.3 + r.3, acc.4 + r.4, acc.5 + r.5));
        let rmse_m = (se_m / ns.max(1) as f64).sqrt();
        let rmse_l = (se_l / ns.max(1) as f64).sqrt();
        let (nz, mag) = nt.stats();
        let proxy = proxy_eval(&w, &pw, &proxy_sp, eval_hands, eval_games, 9_000_000, threads);
        std::fs::write(format!("{}/ntuple_epoch_{:02}.json", out, ep), nt.to_json()).unwrap();
        std::fs::write(format!("{}/latest.json", out), nt.to_json()).unwrap();
        if proxy > best.0 {
            best = (proxy, ep + 1);
            std::fs::write(format!("{}/best.json", out), nt.to_json()).unwrap();
        }
        println!(
            "epoch {:2} | hands={} games={} selfplay_mean={:.0} samples={} rmse(혼합/선형)={:.0}/{:.0} | proxy(혼합, 샘플0)={:.0} (best {:.0} @{}; 선형 {:.0}) | bias={:.0} nonzero={} |v|avg={:.1} | {:.0}s",
            ep + 1, h, g, if g > 0 { sc as f64 / g as f64 } else { 0.0 }, ns, rmse_m, rmse_l, proxy, best.0, best.1, base_proxy, nt.bias(), nz, mag, te.elapsed().as_secs_f64()
        );
    }
    println!("done. best proxy={:.0} (epoch {}) → {}/best.json; 선형={:.0}", best.0, best.1, out, base_proxy);
}

/// 고정 시드 자기대전 평균 점수 (학습 중 스냅샷 비교용 대리 지표).
fn proxy_eval(w: &Weights, pw: &PieceWeights, sp: &SearchParams, max_hands: u32, games: u64, seed: u64, threads: usize) -> f64 {
    let seeds: Vec<u64> = (0..games).map(|k| seed + k).collect();
    let cfg = PlayConfig { max_hands, ..Default::default() };
    let sc = parallel_map(&seeds, threads, |&s| play_game(s, pw, w, sp, &cfg).score as f64);
    sc.iter().sum::<f64>() / games.max(1) as f64
}

/// N-tuple 잔차 항을 TD(0)로 학습한다. 선형 가중치는 고정.
fn cmd_train_td(a: &Args) {
    let threads = a.get("threads", 11usize);
    let epochs = a.get("epochs", 10usize);
    let hands = a.get("hands", 20_000u64);
    let alpha = a.get("alpha", 0.02f32);
    let alpha_bias = a.get("alpha-bias", 0.01f32);
    let gamma = a.get("gamma", 0.95f32);
    let clip = a.get("clip", 2000.0f32);
    // 워밍업 에포크 동안은 선형 V로 행동하고 혼합 V만 학습한다 (초기 전이로 정책이 무너지는 것을 막는다).
    let warmup = a.get("warmup", 4usize);
    // 잘린 MC 수익의 지평 (손패 수). γ^N 이 작아지도록 γ와 맞춘다.
    let horizon = a.get("horizon", 60usize);
    let seed0 = a.get("seed", 7000u64);
    let eval_games = a.get("eval-games", 24u64);
    let eval_hands = a.get("eval-hands", 200u32);
    let out = a.str("out").unwrap_or("weights/runs/td1").to_string();
    std::fs::create_dir_all(&out).expect("출력 폴더");
    let pw = load_pw(a);
    let base = load_weights(a);
    let nt: Arc<NTuple> = match &base.ntuple {
        Some(n) => n.clone(),
        None => Arc::new(NTuple::new(NTuple::default_shapes())),
    };
    let w = Weights { w: base.w, ntuple: Some(nt.clone()) };
    let lin = Weights { w: base.w, ntuple: None };
    let sp = search_params(a);
    let cfg = play_config(a);
    println!("train-td: shapes={:?} entries={} active={} threads={} epochs={} hands/thread={} alpha={} alpha_bias={} gamma={} horizon={} warmup={} beam={} out={}", nt.shapes, nt.len(), nt.active, threads, epochs, hands, alpha, alpha_bias, gamma, horizon, warmup, sp.beam, out);
    let t0 = Instant::now();
    let base_proxy = proxy_eval(&lin, &pw, &sp, eval_hands, eval_games, 9_000_000, threads);
    let start_proxy = proxy_eval(&w, &pw, &sp, eval_hands, eval_games, 9_000_000, threads);
    println!("proxy({}판×{}손패, 빔 {}): 선형만={:.0} 시작(혼합)={:.0} [{:.0}s]", eval_games, eval_hands, sp.beam, base_proxy, start_proxy, t0.elapsed().as_secs_f64());
    let mut best = (start_proxy, 0usize);
    for ep in 0..epochs {
        let te = Instant::now();
        let seeds: Vec<u64> = (0..threads as u64).map(|t| seed0 + (ep as u64) * 1000 + t).collect();
        let act: &Weights = if ep < warmup { &lin } else { &w };
        let res = parallel_map(&seeds, threads, |&s| td_worker(s, hands, act, &w, &pw, &sp, &cfg, alpha, alpha_bias, gamma, clip, horizon));
        let (h, g, sc) = res.iter().fold((0u64, 0u64, 0u64), |acc, r| (acc.0 + r.0, acc.1 + r.1, acc.2 + r.2));
        let (nz, mag) = nt.stats();
        let proxy = proxy_eval(&w, &pw, &sp, eval_hands, eval_games, 9_000_000, threads);
        std::fs::write(format!("{}/ntuple_epoch_{:02}.json", out, ep), nt.to_json()).unwrap();
        std::fs::write(format!("{}/latest.json", out), nt.to_json()).unwrap();
        if proxy > best.0 {
            best = (proxy, ep + 1);
            std::fs::write(format!("{}/best.json", out), nt.to_json()).unwrap();
        }
        println!(
            "epoch {:2}{} | hands={} games={} selfplay_mean={:.0} | proxy={:.0} (best {:.0} @{}) | bias={:.0} nonzero={} |v|avg={:.1} | {:.0}s",
            ep + 1, if ep < warmup { "(warmup)" } else { "" }, h, g, if g > 0 { sc as f64 / g as f64 } else { 0.0 }, proxy, best.0, best.1, nt.bias(), nz, mag, te.elapsed().as_secs_f64()
        );
    }
    println!("done. best proxy={:.0} (epoch {}) → {}/best.json, latest → {}/latest.json; 선형만={:.0}", best.0, best.1, out, out, base_proxy);
}

/// 진단: 선형 정책으로 한 판을 두며 손패 경계마다 V_선형 / V_조회표(바이어스 제외) / 채움 칸 수를 찍는다.
/// 또한 각 손패에서 선형과 혼합이 고른 첫 수가 다른지, 혼합이 고른 afterstate의 두 값은 어떤지 보여준다.
fn cmd_debug_values(a: &Args) {
    let hands = a.get("hands", 40u64);
    let pw = load_pw(a);
    let w = load_weights(a); // --ntuple 포함
    let nt = w.ntuple.clone().expect("--ntuple 필요");
    let lin = Weights { w: w.w, ntuple: None };
    let sp = search_params(a);
    let cfg = play_config(a);
    let mut rng = Rng::new(a.get("seed", 1u64));
    let mut st = State::new_game(&mut rng, &pw);
    println!("bias={:.0}", nt.bias());
    println!("{:>4} {:>6} {:>6} | {:>9} {:>9} | {:>9} {:>9} | {}", "hand", "score", "fill", "Vlin(sel)", "Vnt(sel)", "Vlin(hyb)", "Vnt(hyb)", "same?");
    for h in 0..hands {
        if st.over { println!("game over at hand {}", h); break; }
        let pl = moamoa_engine::search(&st, &pw, &lin, &sp);
        let ph = moamoa_engine::search(&st, &pw, &w, &sp);
        let after = |moves: &[moamoa_engine::Move]| {
            let mut s = st.clone();
            for m in moves { match *m { moamoa_engine::Move::Place { slot, orient, r, c } => { s.place_det(slot as usize, orient as usize, r as usize, c as usize); } moamoa_engine::Move::Dot { r, c } => { s.dot(r as usize, c as usize); } _ => {} } }
            s
        };
        let al = after(&pl.moves);
        let ah = after(&ph.moves);
        let vl = |s: &State| value_full(s, &pw, &lin);
        let vn = |s: &State| nt.eval(&s.board) - nt.bias();
        println!("{:>4} {:>6} {:>6} | {:>9.0} {:>9.0} | {:>9.0} {:>9.0} | {} gain {} vs {}", h, st.score, st.board.filled(), vl(&al), vn(&al), vl(&ah), vn(&ah), if pl.moves.first() == ph.moves.first() { "same" } else { "DIFF" }, pl.gain, ph.gain);
        // 선형 정책으로 진행
        let events = policy_step(&mut st, &mut rng, &pw, &lin, &sp, &cfg);
        if events.is_empty() { break; }
    }
    // 빈 판과 꽉 찬 판의 조회표 값
    let empty = State::blank();
    let mut half = State::blank();
    for r in 8..16 { half.board.rows[r] = 0x3FF ^ (1 << (r % 10)); }
    println!("Vnt(empty)={:.0} Vnt(8 rows missing 1 cell each)={:.0}", nt.eval(&empty.board) - nt.bias(), nt.eval(&half.board) - nt.bias());
}

fn main() {
    let a = parse_args();
    match a.cmd.as_str() {
        "debug-values" => cmd_debug_values(&a),
        "selfplay" => cmd_selfplay(&a),
        "bench" => cmd_bench(&a),
        "train" => cmd_train(&a),
        "train-td" => cmd_train_td(&a),
        "train-distill" => cmd_train_distill(&a),
        "pieces" => cmd_pieces(),
        "weights" => println!("{}", load_weights(&a).to_json()),
        _ => {
            println!("사용법: moamoa <selfplay|bench|train|pieces|weights> [--key value ...]");
            println!("  selfplay --games N --seed S --beam B --threads T [--weights f] [--pw f] [--max-hands M] [--samples K] [--verbose]");
            println!("  train --pop P --elite E --gens G --games N --beam B --max-hands M --out dir [--weights f] [--threads T]");
            println!("  bench --beam B [--samples K]");
            println!("  train-td --weights f [--ntuple f] --epochs E --hands H --alpha A --gamma G --beam B --out dir  (N-tuple 잔차 TD 학습)");
            println!("  selfplay/train 에 --ntuple f 를 주면 혼합 평가 함수를 쓴다");
        }
    }
}
