use std::env;
use std::process::ExitCode;

use qrstatic::codec::temporal::{TemporalConfig, TemporalDecoder, TemporalEncoder};
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
    trials: usize,
    width: usize,
    height: usize,
    frames: usize,
    noise_amplitude: f32,
    l1_amplitude: f32,
    key_prefix: String,
    qr_prefix: String,
}

impl EvalArgs {
    fn parse(mut args: impl Iterator<Item = String>) -> Result<Self, String> {
        let mut parsed = Self {
            trials: 32,
            width: 41,
            height: 41,
            frames: 64,
            noise_amplitude: 0.3,
            l1_amplitude: 0.35,
            key_prefix: "temporal-eval".into(),
            qr_prefix: "temporal-bootstrap".into(),
        };

        while let Some(flag) = args.next() {
            match flag.as_str() {
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
                "--key-prefix" => parsed.key_prefix = next_value(&mut args, "--key-prefix")?,
                "--qr-prefix" => parsed.qr_prefix = next_value(&mut args, "--qr-prefix")?,
                "--help" | "-h" => return Err(help_text()),
                other => return Err(format!("unknown flag: {other}\n\n{}", help_text())),
            }
        }

        if parsed.trials == 0 {
            return Err("--trials must be greater than zero".into());
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

    let mut summary = EvalSummary {
        correct_decode_successes: 0,
        wrong_key_decode_successes: 0,
        wrong_window_decode_successes: 0,
        naive_decode_successes: 0,
        correct_scores: Vec::with_capacity(args.trials),
        wrong_key_scores: Vec::with_capacity(args.trials),
        wrong_window_scores: Vec::with_capacity(args.trials),
        naive_scores: Vec::with_capacity(args.trials),
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

        let correct_decode = decoder.decode_qr(&frames, &master_key).ok();
        if correct_decode
            .as_ref()
            .and_then(|result| result.message.as_deref())
            == Some(qr_payload.as_str())
        {
            summary.correct_decode_successes += 1;
        }

        if decoder.decode_qr(&frames, &wrong_key).is_ok() {
            summary.wrong_key_decode_successes += 1;
        }

        let wrong_window = make_wrong_window(&frames, &other_frames);
        if decoder.decode_qr(&wrong_window, &master_key).is_ok() {
            summary.wrong_window_decode_successes += 1;
        }

        if decoder
            .naive_decode_qr(&frames)
            .map_err(|err| format!("trial {trial}: failed to score naive decode: {err}"))?
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
            decoder
                .naive_score(&frames)
                .map_err(|err| format!("trial {trial}: failed to score naive path: {err}"))?,
        );
    }

    Ok(summary)
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
        "  trials={} frame={}x{} window={} noise_amplitude={:.3} l1_amplitude={:.3}",
        args.trials, args.width, args.height, args.frames, args.noise_amplitude, args.l1_amplitude
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
        "    --trials <count>",
        "    --width <cells>",
        "    --height <cells>",
        "    --frames <count>",
        "    --noise-amplitude <float>",
        "    --l1-amplitude <float>",
        "    --key-prefix <text>",
        "    --qr-prefix <text>",
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
