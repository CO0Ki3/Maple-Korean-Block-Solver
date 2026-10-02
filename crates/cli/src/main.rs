//! moamoa CLI: selfplay / bench / train / pieces
//!
//! 예)
//!   moamoa selfplay --games 20 --beam 32 --threads 12
//!   moamoa train --pop 40 --elite 8 --gens 40 --games 6 --beam 12 --max-hands 150 --out weights/runs/r1
//!   moamoa bench

use moamoa_engine::sim::{play_game, summarize, GameResult, PlayConfig, Summary};
use moamoa_engine::{pieces, PieceWeights, Rng, SearchParams, State, Weights, FEATURE_NAMES, NF};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
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
    match a.str("weights") {
        Some(p) => {
            let s = std::fs::read_to_string(p).unwrap_or_else(|e| panic!("가중치 파일을 읽지 못했습니다 {}: {}", p, e));
            Weights::from_json(&s).unwrap_or_else(|e| panic!("가중치 파싱 실패: {}", e))
        }
        None => Weights::default(),
    }
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

fn main() {
    let a = parse_args();
    match a.cmd.as_str() {
        "selfplay" => cmd_selfplay(&a),
        "bench" => cmd_bench(&a),
        "train" => cmd_train(&a),
        "pieces" => cmd_pieces(),
        "weights" => println!("{}", load_weights(&a).to_json()),
        _ => {
            println!("사용법: moamoa <selfplay|bench|train|pieces|weights> [--key value ...]");
            println!("  selfplay --games N --seed S --beam B --threads T [--weights f] [--pw f] [--max-hands M] [--samples K] [--verbose]");
            println!("  train --pop P --elite E --gens G --games N --beam B --max-hands M --out dir [--weights f] [--threads T]");
            println!("  bench --beam B [--samples K]");
        }
    }
}
