use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use qrstatic::codec::temporal::{
    TemporalConfig, TemporalDecodePolicy, TemporalDecoder, TemporalEncoder, detector_score,
    naive_field, try_extract_qr,
};
use qrstatic::qr;
use qrstatic::Grid;

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
            eprintln!("temporal eval failed: {err}");
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
    frames: usize,
    noise_amplitude: f32,
    l1_amplitude: f32,
    threshold: f32,
    prefix_step: Option<usize>,
    key_prefix: String,
    qr_prefix: String,
    results_tsv: Option<PathBuf>,
}

impl EvalArgs {
    fn parse(mut args: impl Iterator<Item = String>) -> Result<Self, String> {
        let mut parsed = Self {
            profile: "middle-64-a".into(),
            trials: 32,
            width: 41,
            height: 41,
            frames: 64,
            noise_amplitude: 0.42,
            l1_amplitude: 0.22,
            threshold: 6.0,
            prefix_step: None,
            key_prefix: "temporal-eval".into(),
            qr_prefix: "temporal-bootstrap".into(),
            results_tsv: None,
        };

        while let Some(flag) = args.next() {
            match flag.as_str() {
                "--profile" => parsed.profile = next_value(&mut args, "--profile")?,
                "--trials" => parsed.trials = parse_usize(&next_value(&mut args, "--trials")?, "--trials")?,
                "--width" => parsed.width = parse_usize(&next_value(&mut args, "--width")?, "--width")?,
                "--height" => parsed.height = parse_usize(&next_value(&mut args, "--height")?, "--height")?,
                "--frames" => parsed.frames = parse_usize(&next_value(&mut args, "--frames")?, "--frames")?,
                "--noise-amplitude" => {
                    parsed.noise_amplitude =
                        parse_f32(&next_value(&mut args, "--noise-amplitude")?, "--noise-amplitude")?
                }
                "--l1-amplitude" => {
                    parsed.l1_amplitude =
                        parse_f32(&next_value(&mut args, "--l1-amplitude")?, "--l1-amplitude")?
                }
                "--threshold" => {
                    parsed.threshold =
                        parse_f32(&next_value(&mut args, "--threshold")?, "--threshold")?
                }
                "--prefix-step" => {
                    parsed.prefix_step =
                        Some(parse_usize(&next_value(&mut args, "--prefix-step")?, "--prefix-step")?)
                }
                "--key-prefix" => parsed.key_prefix = next_value(&mut args, "--key-prefix")?,
                "--qr-prefix" => parsed.qr_prefix = next_value(&mut args, "--qr-prefix")?,
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
        if matches!(parsed.prefix_step, Some(0)) {
            return Err("--prefix-step must be greater than zero".into());
        }

        Ok(parsed)
    }
}

#[derive(Debug, Clone, PartialEq)]
struct EvalSummary {
    correct_decode_successes: usize,
    wrong_key_decode_successes: usize,
    wrong_window_decode_successes: usize,
    naive_decode_successes: usize,
    correct_scores: Vec<f32>,
    wrong_key_scores: Vec<f32>,
    wrong_window_scores: Vec<f32>,
    naive_scores: Vec<f32>,
    prefix_summaries: Vec<PrefixSummary>,
}

#[derive(Debug, Clone, PartialEq)]
struct PrefixSummary {
    prefix_frames: usize,
    correct_decode_successes: usize,
    wrong_key_decode_successes: usize,
    wrong_window_decode_successes: usize,
    correct_scores: Vec<f32>,
    wrong_key_scores: Vec<f32>,
    wrong_window_scores: Vec<f32>,
}

fn run_eval(args: &EvalArgs) -> Result<EvalSummary, String> {
    let config = TemporalConfig::new(
        (args.width, args.height),
        args.frames,
        args.noise_amplitude,
        args.l1_amplitude,
    )
    .map_err(|err| err.to_string())?;
    let encoder = TemporalEncoder::new(config.clone()).map_err(|err| err.to_string())?;
    let decoder = TemporalDecoder::new(config).map_err(|err| err.to_string())?;
    let policy =
        TemporalDecodePolicy::fixed_threshold(args.threshold).map_err(|err| err.to_string())?;

    let mut summary = EvalSummary {
        correct_decode_successes: 0,
        wrong_key_decode_successes: 0,
        wrong_window_decode_successes: 0,
        naive_decode_successes: 0,
        correct_scores: Vec::with_capacity(args.trials),
        wrong_key_scores: Vec::with_capacity(args.trials),
        wrong_window_scores: Vec::with_capacity(args.trials),
        naive_scores: Vec::with_capacity(args.trials),
        prefix_summaries: build_prefix_summaries(args),
    };

    for trial in 0..args.trials {
        let master_key = format!("{}-{trial}", args.key_prefix);
        let wrong_key = format!("{}-wrong-{trial}", args.key_prefix);
        let qr_payload = format!("{}-{trial}", args.qr_prefix);
        let other_master = format!("{}-other-{trial}", args.key_prefix);
        let other_qr = format!("{}-other-{trial}", args.qr_prefix);

        let frames = encoder
            .encode_message(&master_key, &qr_payload)
            .map_err(|err| format!("trial {trial}: failed to encode primary frames: {err}"))?;
        let other_frames = encoder
            .encode_message(&other_master, &other_qr)
            .map_err(|err| format!("trial {trial}: failed to encode secondary frames: {err}"))?;

        let correct_decode = decoder.decode_qr(&frames, &master_key, &policy).ok();
        if correct_decode
            .as_ref()
            .and_then(|result| result.message.as_deref())
            == Some(qr_payload.as_str())
        {
            summary.correct_decode_successes += 1;
        }

        if decoder.decode_qr(&frames, &wrong_key, &policy).is_ok() {
            summary.wrong_key_decode_successes += 1;
        }

        let wrong_window = make_wrong_window(&frames, &other_frames);
        if decoder.decode_qr(&wrong_window, &master_key, &policy).is_ok() {
            summary.wrong_window_decode_successes += 1;
        }

        if try_extract_qr(
            &naive_field(&frames)
                .map_err(|err| format!("trial {trial}: failed to build naive field: {err}"))?,
        )
        .is_some()
        {
            summary.naive_decode_successes += 1;
        }

        summary.correct_scores.push(
            decoder
                .correlation_score(&frames, &master_key)
                .map_err(|err| format!("trial {trial}: failed to score correct key: {err}"))?,
        );
        summary.wrong_key_scores.push(
            decoder
                .correlation_score(&frames, &wrong_key)
                .map_err(|err| format!("trial {trial}: failed to score wrong key: {err}"))?,
        );
        summary.wrong_window_scores.push(
            decoder
                .correlation_score(&wrong_window, &master_key)
                .map_err(|err| format!("trial {trial}: failed to score wrong window: {err}"))?,
        );
        summary.naive_scores.push(
            detector_score(
                &naive_field(&frames)
                    .map_err(|err| format!("trial {trial}: failed to score naive path: {err}"))?,
            ),
        );

        for prefix_summary in &mut summary.prefix_summaries {
            let prefix_len = prefix_summary.prefix_frames;
            let correct_prefix = &frames[..prefix_len];
            let wrong_window_prefix = &wrong_window[..prefix_len];

            let correct_correlation = decoder
                .correlate_prefix(correct_prefix, &master_key)
                .map_err(|err| format!("trial {trial}: failed to score correct prefix: {err}"))?;
            prefix_summary
                .correct_scores
                .push(correct_correlation.detector_score);
            if prefix_decode_matches(&correct_correlation.field, &qr_payload)
                && correct_correlation.detector_score >= args.threshold
            {
                prefix_summary.correct_decode_successes += 1;
            }

            let wrong_key_correlation = decoder
                .correlate_prefix(correct_prefix, &wrong_key)
                .map_err(|err| format!("trial {trial}: failed to score wrong-key prefix: {err}"))?;
            prefix_summary
                .wrong_key_scores
                .push(wrong_key_correlation.detector_score);
            if wrong_key_correlation.detector_score >= args.threshold
                && prefix_extracts_any_qr(&wrong_key_correlation.field)
            {
                prefix_summary.wrong_key_decode_successes += 1;
            }

            let wrong_window_correlation = decoder
                .correlate_prefix(wrong_window_prefix, &master_key)
                .map_err(|err| format!("trial {trial}: failed to score wrong-window prefix: {err}"))?;
            prefix_summary
                .wrong_window_scores
                .push(wrong_window_correlation.detector_score);
            if wrong_window_correlation.detector_score >= args.threshold
                && prefix_extracts_any_qr(&wrong_window_correlation.field)
            {
                prefix_summary.wrong_window_decode_successes += 1;
            }
        }
    }

    Ok(summary)
}

fn build_prefix_summaries(args: &EvalArgs) -> Vec<PrefixSummary> {
    let Some(step) = args.prefix_step else {
        return Vec::new();
    };

    let mut prefix_frames = Vec::new();
    let mut next = step.min(args.frames);
    while next < args.frames {
        prefix_frames.push(next);
        next += step;
    }
    if prefix_frames.last().copied() != Some(args.frames) {
        prefix_frames.push(args.frames);
    }

    prefix_frames
        .into_iter()
        .map(|prefix_frames| PrefixSummary {
            prefix_frames,
            correct_decode_successes: 0,
            wrong_key_decode_successes: 0,
            wrong_window_decode_successes: 0,
            correct_scores: Vec::with_capacity(args.trials),
            wrong_key_scores: Vec::with_capacity(args.trials),
            wrong_window_scores: Vec::with_capacity(args.trials),
        })
        .collect()
}

fn prefix_decode_matches(field: &Grid<f32>, expected_payload: &str) -> bool {
    try_extract_qr(field)
        .and_then(|qr_grid| qr::decode::decode(&qr_grid).ok())
        .as_deref()
        == Some(expected_payload)
}

fn prefix_extracts_any_qr(field: &Grid<f32>) -> bool {
    try_extract_qr(field)
        .and_then(|qr_grid| qr::decode::decode(&qr_grid).ok())
        .is_some()
}

fn make_wrong_window(frames: &[Grid<f32>], other_frames: &[Grid<f32>]) -> Vec<Grid<f32>> {
    let mut shifted = frames[1..].to_vec();
    shifted.push(other_frames[0].clone());
    shifted
}

fn print_summary(args: &EvalArgs, summary: &EvalSummary) {
    println!("qrstatic-temporal-eval");
    println!();
    println!("config:");
    println!(
        "  profile={} trials={} frame={}x{} window={} noise_amplitude={:.3} l1_amplitude={:.3} threshold={:.3}",
        args.profile,
        args.trials,
        args.width,
        args.height,
        args.frames,
        args.noise_amplitude,
        args.l1_amplitude,
        args.threshold
    );
    println!(
        "  key_prefix={} qr_prefix={}",
        args.key_prefix, args.qr_prefix
    );
    println!();
    println!("decode success:");
    println!(
        "  correct_key   {:>3}/{:<3} ({:>6.2}%)",
        summary.correct_decode_successes,
        args.trials,
        percent(summary.correct_decode_successes, args.trials)
    );
    println!(
        "  wrong_key     {:>3}/{:<3} ({:>6.2}%)",
        summary.wrong_key_decode_successes,
        args.trials,
        percent(summary.wrong_key_decode_successes, args.trials)
    );
    println!(
        "  wrong_window  {:>3}/{:<3} ({:>6.2}%)",
        summary.wrong_window_decode_successes,
        args.trials,
        percent(summary.wrong_window_decode_successes, args.trials)
    );
    println!(
        "  naive_sum     {:>3}/{:<3} ({:>6.2}%)",
        summary.naive_decode_successes,
        args.trials,
        percent(summary.naive_decode_successes, args.trials)
    );
    println!();
    println!("scores:");
    print_score_row("correct_key ", &summary.correct_scores);
    print_score_row("wrong_key   ", &summary.wrong_key_scores);
    print_score_row("wrong_window", &summary.wrong_window_scores);
    print_score_row("naive_sum   ", &summary.naive_scores);
    println!();
    println!("score margins:");
    print_margin_row("correct - wrong_key   ", &summary.correct_scores, &summary.wrong_key_scores);
    print_margin_row(
        "correct - wrong_window",
        &summary.correct_scores,
        &summary.wrong_window_scores,
    );
    print_margin_row("correct - naive_sum   ", &summary.correct_scores, &summary.naive_scores);

    if !summary.prefix_summaries.is_empty() {
        println!();
        println!("prefix acquisition:");
        for prefix_summary in &summary.prefix_summaries {
            let correct_stats = ScoreStats::from_values(&prefix_summary.correct_scores);
            let wrong_key_stats = ScoreStats::from_values(&prefix_summary.wrong_key_scores);
            let wrong_window_stats = ScoreStats::from_values(&prefix_summary.wrong_window_scores);
            println!(
                "  prefix={:>3}/{:<3} correct={:>6.2}% wrong_key={:>6.2}% wrong_window={:>6.2}% scores(correct/wrong/window)={:.3}/{:.3}/{:.3}",
                prefix_summary.prefix_frames,
                args.frames,
                percent(prefix_summary.correct_decode_successes, args.trials),
                percent(prefix_summary.wrong_key_decode_successes, args.trials),
                percent(prefix_summary.wrong_window_decode_successes, args.trials),
                correct_stats.mean,
                wrong_key_stats.mean,
                wrong_window_stats.mean,
            );
        }

        if let Some(k50) = prefix_threshold_frame(&summary.prefix_summaries, args.trials, 50.0) {
            println!("  k50={k50}");
        }
        if let Some(k95) = prefix_threshold_frame(&summary.prefix_summaries, args.trials, 95.0) {
            println!("  k95={k95}");
        }
    }
}

fn prefix_threshold_frame(prefix_summaries: &[PrefixSummary], trials: usize, target_pct: f32) -> Option<usize> {
    prefix_summaries
        .iter()
        .find(|prefix_summary| percent(prefix_summary.correct_decode_successes, trials) >= target_pct)
        .map(|prefix_summary| prefix_summary.prefix_frames)
}

fn print_score_row(label: &str, values: &[f32]) {
    let stats = ScoreStats::from_values(values);
    println!(
        "  {} mean={:>7.3} min={:>7.3} max={:>7.3}",
        label, stats.mean, stats.min, stats.max
    );
}

fn print_margin_row(label: &str, left: &[f32], right: &[f32]) {
    let margins: Vec<f32> = left.iter().zip(right.iter()).map(|(a, b)| a - b).collect();
    let stats = ScoreStats::from_values(&margins);
    println!(
        "  {} mean={:>7.3} min={:>7.3} max={:>7.3}",
        label, stats.mean, stats.min, stats.max
    );
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct ScoreStats {
    mean: f32,
    min: f32,
    max: f32,
}

impl ScoreStats {
    fn from_values(values: &[f32]) -> Self {
        let mean = values.iter().sum::<f32>() / values.len() as f32;
        let min = values.iter().copied().min_by(f32::total_cmp).unwrap_or(0.0);
        let max = values.iter().copied().max_by(f32::total_cmp).unwrap_or(0.0);
        Self { mean, min, max }
    }
}

fn percent(count: usize, total: usize) -> f32 {
    (count as f32 * 100.0) / total as f32
}

fn append_results_tsv(
    path: &PathBuf,
    args: &EvalArgs,
    summary: &EvalSummary,
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .map_err(|err| format!("failed to create {}: {err}", parent.display()))?;
        }
    }

    let needs_header = match fs::metadata(path) {
        Ok(metadata) => metadata.len() == 0,
        Err(_) => true,
    };

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|err| format!("failed to open {}: {err}", path.display()))?;

    if needs_header {
        writeln!(
            file,
            "profile\ttrials\twidth\theight\tframes\tnoise_amplitude\tl1_amplitude\tthreshold\tcorrect_decode_pct\twrong_key_pct\twrong_window_pct\tnaive_decode_pct\tcorrect_score_mean\twrong_key_score_mean\twrong_window_score_mean\tnaive_score_mean\tcorrect_wrong_key_margin_mean\tcorrect_wrong_window_margin_mean\tcorrect_naive_margin_mean\tkey_prefix\tqr_prefix"
        )
        .map_err(|err| format!("failed to write header to {}: {err}", path.display()))?;
    }

    let correct_stats = ScoreStats::from_values(&summary.correct_scores);
    let wrong_key_stats = ScoreStats::from_values(&summary.wrong_key_scores);
    let wrong_window_stats = ScoreStats::from_values(&summary.wrong_window_scores);
    let naive_stats = ScoreStats::from_values(&summary.naive_scores);
    let correct_wrong_key_margins: Vec<f32> = summary
        .correct_scores
        .iter()
        .zip(summary.wrong_key_scores.iter())
        .map(|(a, b)| a - b)
        .collect();
    let correct_wrong_window_margins: Vec<f32> = summary
        .correct_scores
        .iter()
        .zip(summary.wrong_window_scores.iter())
        .map(|(a, b)| a - b)
        .collect();
    let correct_naive_margins: Vec<f32> = summary
        .correct_scores
        .iter()
        .zip(summary.naive_scores.iter())
        .map(|(a, b)| a - b)
        .collect();

    writeln!(
        file,
        "{}\t{}\t{}\t{}\t{}\t{:.3}\t{:.3}\t{:.3}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{}\t{}",
        args.profile,
        args.trials,
        args.width,
        args.height,
        args.frames,
        args.noise_amplitude,
        args.l1_amplitude,
        args.threshold,
        percent(summary.correct_decode_successes, args.trials),
        percent(summary.wrong_key_decode_successes, args.trials),
        percent(summary.wrong_window_decode_successes, args.trials),
        percent(summary.naive_decode_successes, args.trials),
        correct_stats.mean,
        wrong_key_stats.mean,
        wrong_window_stats.mean,
        naive_stats.mean,
        ScoreStats::from_values(&correct_wrong_key_margins).mean,
        ScoreStats::from_values(&correct_wrong_window_margins).mean,
        ScoreStats::from_values(&correct_naive_margins).mean,
        args.key_prefix,
        args.qr_prefix
    )
    .map_err(|err| format!("failed to append row to {}: {err}", path.display()))?;

    Ok(())
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

fn parse_f32(value: &str, flag: &str) -> Result<f32, String> {
    value
        .parse::<f32>()
        .map_err(|_| format!("invalid f32 for {flag}: {value}"))
}

fn help_text() -> String {
    [
        "qrstatic-temporal-eval",
        "",
        "USAGE:",
        "    cargo run -p qrstatic-cli --bin qrstatic-temporal-eval -- [options]",
        "",
        "OPTIONS:",
        "    --profile <name>",
        "    --trials <count>",
        "    --width <cells>",
        "    --height <cells>",
        "    --frames <count>",
        "    --noise-amplitude <float>",
        "    --l1-amplitude <float>",
        "    --threshold <float>",
        "    --prefix-step <count>",
        "    --key-prefix <text>",
        "    --qr-prefix <text>",
        "    --results-tsv <path>",
    ]
    .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn score_stats_handles_basic_series() {
        let stats = ScoreStats::from_values(&[1.0, 2.0, 3.0]);
        assert_eq!(stats.mean, 2.0);
        assert_eq!(stats.min, 1.0);
        assert_eq!(stats.max, 3.0);
    }
}
