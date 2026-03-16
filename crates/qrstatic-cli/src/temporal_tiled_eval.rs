use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use qrstatic::codec::temporal::TemporalDecodePolicy;
use qrstatic::codec::temporal_tiled::{
    TiledConfig, TiledDecodeResult, TiledDecoder, TiledEncoder, TiledStreamBlock,
};

fn main() -> ExitCode {
    let args = match EvalArgs::parse(env::args().skip(1)) {
        Ok(args) => args,
        Err(err) => {
            eprintln!("{err}");
            return ExitCode::from(2);
        }
    };

    match run_eval(&args) {
        Ok(summary) => {
            print_summary(&args, &summary);
            if let Some(path) = &args.results_tsv {
                if let Err(err) = append_results_tsv(path, &args, &summary) {
                    eprintln!("failed to append results tsv: {err}");
                    return ExitCode::from(1);
                }
                println!();
                println!("results_tsv: {}", path.display());
            }
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("temporal tiled eval failed: {err}");
            ExitCode::from(1)
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct EvalArgs {
    profile: String,
    trials: usize,
    width: usize,
    height: usize,
    qr_version: u8,
    frames: usize,
    noise_amplitude: f32,
    l1_amplitude: f32,
    threshold: f32,
    data_shards: usize,
    parity_shards: usize,
    payload_bytes: usize,
    session_id: u64,
    key_prefix: String,
    results_tsv: Option<PathBuf>,
}

impl EvalArgs {
    fn parse(mut args: impl Iterator<Item = String>) -> Result<Self, String> {
        let mut parsed = Self {
            profile: "tiled-v3-mid".into(),
            trials: 16,
            width: 638,
            height: 464,
            qr_version: 3,
            frames: 64,
            noise_amplitude: 0.42,
            l1_amplitude: 0.22,
            threshold: 6.0,
            data_shards: 3,
            parity_shards: 2,
            payload_bytes: 512,
            session_id: 1,
            key_prefix: "temporal-tiled-eval".into(),
            results_tsv: None,
        };

        while let Some(flag) = args.next() {
            match flag.as_str() {
                "--profile" => parsed.profile = next_value(&mut args, "--profile")?,
                "--trials" => {
                    parsed.trials = parse_usize(&next_value(&mut args, "--trials")?, "--trials")?
                }
                "--width" => {
                    parsed.width = parse_usize(&next_value(&mut args, "--width")?, "--width")?
                }
                "--height" => {
                    parsed.height = parse_usize(&next_value(&mut args, "--height")?, "--height")?
                }
                "--qr-version" => {
                    parsed.qr_version =
                        parse_u8(&next_value(&mut args, "--qr-version")?, "--qr-version")?
                }
                "--frames" => {
                    parsed.frames = parse_usize(&next_value(&mut args, "--frames")?, "--frames")?
                }
                "--noise-amplitude" => {
                    parsed.noise_amplitude = parse_f32(
                        &next_value(&mut args, "--noise-amplitude")?,
                        "--noise-amplitude",
                    )?
                }
                "--l1-amplitude" => {
                    parsed.l1_amplitude =
                        parse_f32(&next_value(&mut args, "--l1-amplitude")?, "--l1-amplitude")?
                }
                "--threshold" => {
                    parsed.threshold =
                        parse_f32(&next_value(&mut args, "--threshold")?, "--threshold")?
                }
                "--data-shards" => {
                    parsed.data_shards =
                        parse_usize(&next_value(&mut args, "--data-shards")?, "--data-shards")?
                }
                "--parity-shards" => {
                    parsed.parity_shards = parse_usize(
                        &next_value(&mut args, "--parity-shards")?,
                        "--parity-shards",
                    )?
                }
                "--payload-bytes" => {
                    parsed.payload_bytes = parse_usize(
                        &next_value(&mut args, "--payload-bytes")?,
                        "--payload-bytes",
                    )?
                }
                "--session-id" => {
                    parsed.session_id =
                        parse_u64(&next_value(&mut args, "--session-id")?, "--session-id")?
                }
                "--key-prefix" => parsed.key_prefix = next_value(&mut args, "--key-prefix")?,
                "--results-tsv" => {
                    parsed.results_tsv =
                        Some(PathBuf::from(next_value(&mut args, "--results-tsv")?))
                }
                "--help" | "-h" => return Err(help_text()),
                other => return Err(format!("unknown flag: {other}\n\n{}", help_text())),
            }
        }

        if parsed.trials == 0 {
            return Err("--trials must be greater than zero".into());
        }
        if parsed.payload_bytes == 0 {
            return Err("--payload-bytes must be greater than zero".into());
        }

        Ok(parsed)
    }
}

#[derive(Debug, Clone, PartialEq)]
struct EvalSummary {
    tiles_total: usize,
    active_tiles: usize,
    tiles_x: usize,
    tiles_y: usize,
    dead_x: usize,
    dead_y: usize,
    shard_data_bytes: usize,
    max_payload_bytes: usize,
    payload_capacity_ok: bool,
    full_block_successes: usize,
    wrong_key_block_successes: usize,
    wrong_window_block_successes: usize,
    mean_tiles_decoded: f32,
    mean_groups_recovered: f32,
    mean_correct_tile_score: f32,
    mean_wrong_key_tile_score: f32,
    mean_wrong_window_tile_score: f32,
}

fn run_eval(args: &EvalArgs) -> Result<EvalSummary, String> {
    let config = TiledConfig::new(
        (args.width, args.height),
        args.qr_version,
        args.frames,
        args.noise_amplitude,
        args.l1_amplitude,
        args.data_shards,
        args.parity_shards,
    )
    .map_err(|err| err.to_string())?;
    let encoder =
        TiledEncoder::new(config.clone(), &args.key_prefix).map_err(|err| err.to_string())?;
    let decoder = TiledDecoder::new(config, &args.key_prefix).map_err(|err| err.to_string())?;
    let policy =
        TemporalDecodePolicy::fixed_threshold(args.threshold).map_err(|err| err.to_string())?;
    let layout = encoder.layout().clone();

    let payload_capacity_ok = args.payload_bytes + 29 <= layout.max_payload_bytes;
    if !payload_capacity_ok {
        return Err(format!(
            "requested payload {} bytes exceeds usable tiled stream-block capacity {} bytes",
            args.payload_bytes,
            layout.max_payload_bytes.saturating_sub(29)
        ));
    }

    let mut full_block_successes = 0usize;
    let mut wrong_key_block_successes = 0usize;
    let mut wrong_window_block_successes = 0usize;
    let mut tiles_decoded_sum = 0usize;
    let mut groups_recovered_sum = 0usize;
    let mut correct_tile_score_sum = 0.0f32;
    let mut wrong_key_tile_score_sum = 0.0f32;
    let mut wrong_window_tile_score_sum = 0.0f32;
    let mut correct_tile_score_count = 0usize;
    let mut wrong_key_tile_score_count = 0usize;
    let mut wrong_window_tile_score_count = 0usize;

    for trial in 0..args.trials {
        let master_key = format!("{}-{trial}", args.key_prefix);
        let wrong_key = format!("{}-wrong-{trial}", args.key_prefix);
        let other_key = format!("{}-other-{trial}", args.key_prefix);

        let trial_encoder = TiledEncoder::new(encoder.config().clone(), &master_key)
            .map_err(|err| err.to_string())?;
        let trial_decoder = TiledDecoder::new(decoder.config().clone(), &master_key)
            .map_err(|err| err.to_string())?;

        let payload = build_payload(args.payload_bytes, trial);
        let block =
            TiledStreamBlock::new(args.session_id, trial as u32, args.trials as u32, payload)
                .map_err(|err| err.to_string())?;
        let frames = trial_encoder
            .encode_stream_block(&master_key, &block)
            .map_err(|err| format!("trial {trial}: failed to encode stream block: {err}"))?;

        let decode_result = trial_decoder
            .decode_payload(&frames, &master_key, &policy)
            .map_err(|err| format!("trial {trial}: failed to decode correct block: {err}"))?;
        accumulate_tile_stats(
            &decode_result,
            &mut tiles_decoded_sum,
            &mut groups_recovered_sum,
            &mut correct_tile_score_sum,
            &mut correct_tile_score_count,
        );
        if decode_result.stream_block.as_ref() == Some(&block) {
            full_block_successes += 1;
        }

        let wrong_key_decoder = TiledDecoder::new(decoder.config().clone(), &wrong_key)
            .map_err(|err| err.to_string())?;
        let wrong_key_result = wrong_key_decoder
            .decode_payload(&frames, &wrong_key, &policy)
            .map_err(|err| format!("trial {trial}: failed to decode wrong-key block: {err}"))?;
        accumulate_score_only(
            &wrong_key_result,
            &mut wrong_key_tile_score_sum,
            &mut wrong_key_tile_score_count,
        );
        if wrong_key_result.stream_block.is_some() {
            wrong_key_block_successes += 1;
        }

        let other_encoder = TiledEncoder::new(encoder.config().clone(), &other_key)
            .map_err(|err| err.to_string())?;
        let other_payload = build_payload(args.payload_bytes, trial + 10_000);
        let other_block = TiledStreamBlock::new(
            args.session_id.wrapping_add(1),
            trial as u32,
            args.trials as u32,
            other_payload,
        )
        .map_err(|err| err.to_string())?;
        let other_frames = other_encoder
            .encode_stream_block(&other_key, &other_block)
            .map_err(|err| format!("trial {trial}: failed to encode other block: {err}"))?;
        let wrong_window = make_wrong_window(&frames, &other_frames);
        let wrong_window_result = trial_decoder
            .decode_payload(&wrong_window, &master_key, &policy)
            .map_err(|err| format!("trial {trial}: failed to decode wrong-window block: {err}"))?;
        accumulate_score_only(
            &wrong_window_result,
            &mut wrong_window_tile_score_sum,
            &mut wrong_window_tile_score_count,
        );
        if wrong_window_result.stream_block.is_some() {
            wrong_window_block_successes += 1;
        }
    }

    Ok(EvalSummary {
        tiles_total: layout.total_tiles,
        active_tiles: layout.active_tiles,
        tiles_x: layout.tiles_x,
        tiles_y: layout.tiles_y,
        dead_x: layout.dead_x,
        dead_y: layout.dead_y,
        shard_data_bytes: layout.shard_data_bytes,
        max_payload_bytes: layout.max_payload_bytes,
        payload_capacity_ok,
        full_block_successes,
        wrong_key_block_successes,
        wrong_window_block_successes,
        mean_tiles_decoded: tiles_decoded_sum as f32 / args.trials as f32,
        mean_groups_recovered: groups_recovered_sum as f32 / args.trials as f32,
        mean_correct_tile_score: mean(correct_tile_score_sum, correct_tile_score_count),
        mean_wrong_key_tile_score: mean(wrong_key_tile_score_sum, wrong_key_tile_score_count),
        mean_wrong_window_tile_score: mean(
            wrong_window_tile_score_sum,
            wrong_window_tile_score_count,
        ),
    })
}

fn accumulate_tile_stats(
    result: &TiledDecodeResult,
    tiles_decoded_sum: &mut usize,
    groups_recovered_sum: &mut usize,
    tile_score_sum: &mut f32,
    tile_score_count: &mut usize,
) {
    *tiles_decoded_sum += result.tiles_decoded;
    *groups_recovered_sum += result
        .group_results
        .iter()
        .filter(|group| group.recovered)
        .count();

    for tile_result in &result.tile_results {
        match tile_result {
            qrstatic::codec::temporal_tiled::TileDecodeOutcome::Success {
                detector_score, ..
            }
            | qrstatic::codec::temporal_tiled::TileDecodeOutcome::Failed { detector_score } => {
                if *detector_score > 0.0 {
                    *tile_score_sum += *detector_score;
                    *tile_score_count += 1;
                }
            }
        }
    }
}

fn accumulate_score_only(
    result: &TiledDecodeResult,
    tile_score_sum: &mut f32,
    tile_score_count: &mut usize,
) {
    for tile_result in &result.tile_results {
        match tile_result {
            qrstatic::codec::temporal_tiled::TileDecodeOutcome::Success {
                detector_score, ..
            }
            | qrstatic::codec::temporal_tiled::TileDecodeOutcome::Failed { detector_score } => {
                if *detector_score > 0.0 {
                    *tile_score_sum += *detector_score;
                    *tile_score_count += 1;
                }
            }
        }
    }
}

fn mean(sum: f32, count: usize) -> f32 {
    if count == 0 { 0.0 } else { sum / count as f32 }
}

fn build_payload(payload_bytes: usize, seed: usize) -> Vec<u8> {
    (0..payload_bytes)
        .map(|index| ((index + seed * 31) & 0xff) as u8)
        .collect()
}

fn make_wrong_window(
    frames: &[qrstatic::Grid<f32>],
    other_frames: &[qrstatic::Grid<f32>],
) -> Vec<qrstatic::Grid<f32>> {
    let mut shifted = frames[1..].to_vec();
    shifted.push(other_frames[0].clone());
    shifted
}

fn print_summary(args: &EvalArgs, summary: &EvalSummary) {
    println!("qrstatic-temporal-tiled-eval");
    println!();
    println!("profile: {}", args.profile);
    println!(
        "video: {}x{}  qr_version={}  frames={}  trials={}",
        args.width, args.height, args.qr_version, args.frames, args.trials
    );
    println!(
        "temporal: noise={:.3} l1={:.3} threshold={:.3}",
        args.noise_amplitude, args.l1_amplitude, args.threshold
    );
    println!(
        "rs: data_shards={} parity_shards={} payload_bytes={}",
        args.data_shards, args.parity_shards, args.payload_bytes
    );
    println!();
    println!(
        "layout: {}x{} tiles={} active={} dead={}x{} shard_data_bytes={} max_payload_bytes={}",
        summary.tiles_x,
        summary.tiles_y,
        summary.tiles_total,
        summary.active_tiles,
        summary.dead_x,
        summary.dead_y,
        summary.shard_data_bytes,
        summary.max_payload_bytes
    );
    println!();
    println!(
        "block success: correct {}/{} wrong-key {}/{} wrong-window {}/{}",
        summary.full_block_successes,
        args.trials,
        summary.wrong_key_block_successes,
        args.trials,
        summary.wrong_window_block_successes,
        args.trials
    );
    println!(
        "mean tiles decoded: {:.2} / {}",
        summary.mean_tiles_decoded, summary.active_tiles
    );
    println!(
        "mean groups recovered: {:.2} / {}",
        summary.mean_groups_recovered,
        summary.active_tiles / (args.data_shards + args.parity_shards)
    );
    println!(
        "mean tile detector scores: correct {:.3} wrong-key {:.3} wrong-window {:.3}",
        summary.mean_correct_tile_score,
        summary.mean_wrong_key_tile_score,
        summary.mean_wrong_window_tile_score
    );
}

fn append_results_tsv(
    path: &PathBuf,
    args: &EvalArgs,
    summary: &EvalSummary,
) -> Result<(), String> {
    let parent = path.parent().unwrap_or_else(|| std::path::Path::new("."));
    fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    let exists = path.exists();
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|err| err.to_string())?;

    if !exists {
        writeln!(
            file,
            "profile\ttrials\twidth\theight\tqr_version\tframes\tnoise_amplitude\tl1_amplitude\tthreshold\tdata_shards\tparity_shards\tpayload_bytes\ttiles_x\ttiles_y\ttiles_total\tactive_tiles\tdead_x\tdead_y\tshard_data_bytes\tmax_payload_bytes\tfull_block_successes\twrong_key_block_successes\twrong_window_block_successes\tmean_tiles_decoded\tmean_groups_recovered\tmean_correct_tile_score\tmean_wrong_key_tile_score\tmean_wrong_window_tile_score"
        )
        .map_err(|err| err.to_string())?;
    }

    writeln!(
        file,
        "{}\t{}\t{}\t{}\t{}\t{}\t{:.6}\t{:.6}\t{:.6}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{:.6}\t{:.6}\t{:.6}\t{:.6}\t{:.6}",
        args.profile,
        args.trials,
        args.width,
        args.height,
        args.qr_version,
        args.frames,
        args.noise_amplitude,
        args.l1_amplitude,
        args.threshold,
        args.data_shards,
        args.parity_shards,
        args.payload_bytes,
        summary.tiles_x,
        summary.tiles_y,
        summary.tiles_total,
        summary.active_tiles,
        summary.dead_x,
        summary.dead_y,
        summary.shard_data_bytes,
        summary.max_payload_bytes,
        summary.full_block_successes,
        summary.wrong_key_block_successes,
        summary.wrong_window_block_successes,
        summary.mean_tiles_decoded,
        summary.mean_groups_recovered,
        summary.mean_correct_tile_score,
        summary.mean_wrong_key_tile_score,
        summary.mean_wrong_window_tile_score
    )
    .map_err(|err| err.to_string())
}

fn next_value(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, String> {
    args.next()
        .ok_or_else(|| format!("missing value for {flag}"))
}

fn parse_usize(value: &str, flag: &str) -> Result<usize, String> {
    value
        .parse::<usize>()
        .map_err(|_| format!("invalid usize for {flag}: {value}"))
}

fn parse_u8(value: &str, flag: &str) -> Result<u8, String> {
    value
        .parse::<u8>()
        .map_err(|_| format!("invalid u8 for {flag}: {value}"))
}

fn parse_u64(value: &str, flag: &str) -> Result<u64, String> {
    value
        .parse::<u64>()
        .map_err(|_| format!("invalid u64 for {flag}: {value}"))
}

fn parse_f32(value: &str, flag: &str) -> Result<f32, String> {
    value
        .parse::<f32>()
        .map_err(|_| format!("invalid f32 for {flag}: {value}"))
}

fn help_text() -> String {
    [
        "qrstatic-temporal-tiled-eval",
        "",
        "USAGE:",
        "    cargo run -p qrstatic-cli --bin qrstatic-temporal-tiled-eval -- [options]",
        "",
        "OPTIONS:",
        "    --profile <name>",
        "    --trials <count>",
        "    --width <cells>",
        "    --height <cells>",
        "    --qr-version <1-6>",
        "    --frames <count>",
        "    --noise-amplitude <float>",
        "    --l1-amplitude <float>",
        "    --threshold <float>",
        "    --data-shards <count>",
        "    --parity-shards <count>",
        "    --payload-bytes <count>",
        "    --session-id <u64>",
        "    --key-prefix <text>",
        "    --results-tsv <path>",
    ]
    .join("\n")
}
