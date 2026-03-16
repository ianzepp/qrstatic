mod binary_container;

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use binary_container::BinaryContainer;
use qrstatic::codec::binary::{BinaryDecoder, BinaryEncoder};
use qrstatic::qr;

const DEFAULT_WIDTH: usize = 41;
const DEFAULT_HEIGHT: usize = 41;
const DEFAULT_FRAMES: usize = 60;
const DEFAULT_BASE_BIAS: f32 = 0.8;
const DEFAULT_PAYLOAD_BIAS_DELTA: f32 = 0.1;
const DEFAULT_SEED: &str = "qrstatic-cli";

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("encode") => handle_encode(args),
        Some("decode") => handle_decode(args),
        Some("help") | Some("--help") | Some("-h") | None => {
            print_help();
            ExitCode::SUCCESS
        }
        Some("--version") | Some("-V") => {
            println!("qrstatic {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("unknown subcommand: {other}");
            print_help();
            ExitCode::from(2)
        }
    }
}

fn handle_encode(mut args: impl Iterator<Item = String>) -> ExitCode {
    match args.next().as_deref() {
        Some("binary") => encode_binary(args),
        Some(codec) => {
            eprintln!("unsupported encode codec: {codec}");
            ExitCode::from(2)
        }
        None => {
            eprintln!("missing codec for 'encode'");
            print_help();
            ExitCode::from(2)
        }
    }
}

fn handle_decode(mut args: impl Iterator<Item = String>) -> ExitCode {
    match args.next().as_deref() {
        Some("binary") => decode_binary(args),
        Some(codec) => {
            eprintln!("unsupported decode codec: {codec}");
            ExitCode::from(2)
        }
        None => {
            eprintln!("missing codec for 'decode'");
            print_help();
            ExitCode::from(2)
        }
    }
}

fn print_help() {
    println!("qrstatic");
    println!();
    println!("USAGE:");
    println!("    qrstatic <SUBCOMMAND>");
    println!();
    println!("SUBCOMMANDS:");
    println!("    encode <codec>    Encode a payload into a carrier stream");
    println!("    decode <codec>    Decode a payload from a carrier stream");
    println!("    help              Print this help text");
    println!();
    println!("BINARY ENCODE:");
    println!("    qrstatic encode binary --qr-key <key> --payload-text <text> --out <file>");
    println!("    qrstatic encode binary --qr-key <key> --payload-file <path> --out <file>");
    println!("    add --optimize to search for the smallest viable binary stream settings");
    println!();
    println!("BINARY DECODE:");
    println!("    qrstatic decode binary --in <file> [--payload-out <path>]");
}

fn encode_binary(args: impl Iterator<Item = String>) -> ExitCode {
    let parsed = match BinaryEncodeArgs::parse(args) {
        Ok(parsed) => parsed,
        Err(err) => {
            eprintln!("{err}");
            return ExitCode::from(2);
        }
    };

    let payload = match parsed.read_payload() {
        Ok(payload) => payload,
        Err(err) => {
            eprintln!("{err}");
            return ExitCode::from(1);
        }
    };

    let effective = match resolve_binary_encode_config(&parsed, &payload) {
        Ok(effective) => effective,
        Err(err) => {
            eprintln!("{err}");
            return ExitCode::from(1);
        }
    };

    let encoder = match BinaryEncoder::new(
        effective.n_frames,
        (effective.width, effective.height),
        parsed.seed.clone(),
        parsed.base_bias,
        parsed.payload_bias_delta,
    ) {
        Ok(encoder) => encoder,
        Err(err) => {
            eprintln!("failed to create binary encoder: {err}");
            return ExitCode::from(1);
        }
    };

    let frames = match encoder.encode_message(&parsed.qr_key, &payload) {
        Ok(frames) => frames,
        Err(err) => {
            eprintln!("failed to encode payload: {err}");
            return ExitCode::from(1);
        }
    };

    let container = BinaryContainer {
        width: effective.width,
        height: effective.height,
        n_frames: effective.n_frames,
        seed: parsed.seed,
        base_bias: parsed.base_bias,
        payload_bias_delta: parsed.payload_bias_delta,
        payload_len: payload.len(),
        packed_bits: parsed.optimize,
        frames,
    };

    if let Err(err) = container.write_to_path(&parsed.out) {
        eprintln!("{err}");
        return ExitCode::from(1);
    }

    println!(
        "encoded {} bytes into {} binary frames at {}x{}",
        payload.len(),
        effective.n_frames,
        effective.width,
        effective.height
    );
    println!("container: {}", parsed.out.display());
    ExitCode::SUCCESS
}

fn decode_binary(args: impl Iterator<Item = String>) -> ExitCode {
    let parsed = match BinaryDecodeArgs::parse(args) {
        Ok(parsed) => parsed,
        Err(err) => {
            eprintln!("{err}");
            return ExitCode::from(2);
        }
    };

    let container = match BinaryContainer::read_from_path(&parsed.input) {
        Ok(container) => container,
        Err(err) => {
            eprintln!("{err}");
            return ExitCode::from(1);
        }
    };

    let decoder = match BinaryDecoder::new(container.payload_len, container.base_bias) {
        Ok(decoder) => decoder,
        Err(err) => {
            eprintln!("failed to create binary decoder: {err}");
            return ExitCode::from(1);
        }
    };

    let result = match decoder.decode_message(&container.frames) {
        Ok(result) => result,
        Err(err) => {
            eprintln!("failed to decode payload: {err}");
            return ExitCode::from(1);
        }
    };

    let qr_key = match result.message {
        Some(message) => message,
        None => {
            eprintln!("no QR message was recoverable from the accumulated frames");
            return ExitCode::from(1);
        }
    };

    let payload = match result.payload {
        Some(payload) => payload,
        None => {
            eprintln!("payload was not recoverable from the decoded frame window");
            return ExitCode::from(1);
        }
    };

    println!("recovered qr-key: {qr_key}");
    println!("recovered payload bytes: {}", payload.len());

    if let Some(path) = parsed.payload_out {
        if let Err(err) = fs::write(&path, &payload) {
            eprintln!("failed to write {}: {err}", path.display());
            return ExitCode::from(1);
        }
        println!("payload written to {}", path.display());
    } else if let Ok(text) = std::str::from_utf8(&payload) {
        println!("payload text: {text}");
    } else {
        println!("payload is binary; pass --payload-out <path> to write it");
    }

    ExitCode::SUCCESS
}

#[derive(Debug, Clone, PartialEq)]
struct BinaryEncodeArgs {
    qr_key: String,
    payload_text: Option<String>,
    payload_file: Option<PathBuf>,
    out: PathBuf,
    width: Option<usize>,
    height: Option<usize>,
    n_frames: Option<usize>,
    seed: String,
    base_bias: f32,
    payload_bias_delta: f32,
    optimize: bool,
}

impl BinaryEncodeArgs {
    fn parse(mut args: impl Iterator<Item = String>) -> Result<Self, String> {
        let mut qr_key = None;
        let mut payload_text = None;
        let mut payload_file = None;
        let mut out = None;
        let mut width = None;
        let mut height = None;
        let mut n_frames = None;
        let mut seed = DEFAULT_SEED.to_string();
        let mut base_bias = DEFAULT_BASE_BIAS;
        let mut payload_bias_delta = DEFAULT_PAYLOAD_BIAS_DELTA;
        let mut optimize = false;

        while let Some(flag) = args.next() {
            match flag.as_str() {
                "--qr-key" => qr_key = Some(next_value(&mut args, "--qr-key")?),
                "--payload-text" => payload_text = Some(next_value(&mut args, "--payload-text")?),
                "--payload-file" => {
                    payload_file = Some(PathBuf::from(next_value(&mut args, "--payload-file")?))
                }
                "--out" => out = Some(PathBuf::from(next_value(&mut args, "--out")?)),
                "--width" => {
                    width = Some(parse_usize(&next_value(&mut args, "--width")?, "--width")?)
                }
                "--height" => {
                    height = Some(parse_usize(
                        &next_value(&mut args, "--height")?,
                        "--height",
                    )?)
                }
                "--frames" => {
                    n_frames = Some(parse_usize(
                        &next_value(&mut args, "--frames")?,
                        "--frames",
                    )?)
                }
                "--seed" => seed = next_value(&mut args, "--seed")?,
                "--base-bias" => {
                    base_bias = parse_f32(&next_value(&mut args, "--base-bias")?, "--base-bias")?
                }
                "--payload-bias-delta" => {
                    payload_bias_delta = parse_f32(
                        &next_value(&mut args, "--payload-bias-delta")?,
                        "--payload-bias-delta",
                    )?
                }
                "--optimize" => optimize = true,
                other => return Err(format!("unknown encode flag: {other}")),
            }
        }

        let qr_key = qr_key.ok_or_else(|| "missing required flag --qr-key".to_string())?;
        let out = out.ok_or_else(|| "missing required flag --out".to_string())?;
        let payload_sources =
            usize::from(payload_text.is_some()) + usize::from(payload_file.is_some());
        if payload_sources != 1 {
            return Err(
                "provide exactly one of --payload-text or --payload-file for binary encode".into(),
            );
        }

        Ok(Self {
            qr_key,
            payload_text,
            payload_file,
            out,
            width,
            height,
            n_frames,
            seed,
            base_bias,
            payload_bias_delta,
            optimize,
        })
    }

    fn read_payload(&self) -> Result<Vec<u8>, String> {
        match (&self.payload_text, &self.payload_file) {
            (Some(text), None) => Ok(text.as_bytes().to_vec()),
            (None, Some(path)) => {
                fs::read(path).map_err(|err| format!("failed to read {}: {err}", path.display()))
            }
            _ => Err("binary encode requires exactly one payload source".into()),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct BinaryDecodeArgs {
    input: PathBuf,
    payload_out: Option<PathBuf>,
}

impl BinaryDecodeArgs {
    fn parse(mut args: impl Iterator<Item = String>) -> Result<Self, String> {
        let mut input = None;
        let mut payload_out = None;

        while let Some(flag) = args.next() {
            match flag.as_str() {
                "--in" => input = Some(PathBuf::from(next_value(&mut args, "--in")?)),
                "--payload-out" => {
                    payload_out = Some(PathBuf::from(next_value(&mut args, "--payload-out")?))
                }
                other => return Err(format!("unknown decode flag: {other}")),
            }
        }

        Ok(Self {
            input: input.ok_or_else(|| "missing required flag --in".to_string())?,
            payload_out,
        })
    }
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

#[derive(Debug, Clone, PartialEq)]
struct EffectiveBinaryEncodeConfig {
    width: usize,
    height: usize,
    n_frames: usize,
}

fn resolve_binary_encode_config(
    args: &BinaryEncodeArgs,
    payload: &[u8],
) -> Result<EffectiveBinaryEncodeConfig, String> {
    let qr_grid = qr::encode::encode(&args.qr_key)
        .map_err(|err| format!("failed to encode qr-key as QR payload: {err}"))?;
    let qr_size = qr_grid.width().max(qr_grid.height());
    let payload_bits = payload
        .len()
        .checked_mul(8)
        .ok_or_else(|| "payload bit length overflow".to_string())?;

    if args.optimize {
        return optimize_binary_config(args, payload, payload_bits, qr_size);
    }

    let width = args.width.unwrap_or(DEFAULT_WIDTH);
    let height = args.height.unwrap_or(DEFAULT_HEIGHT);
    let n_frames = args.n_frames.unwrap_or(DEFAULT_FRAMES);
    validate_binary_capacity(width, height, payload_bits, qr_size)?;

    Ok(EffectiveBinaryEncodeConfig {
        width,
        height,
        n_frames,
    })
}

fn optimize_binary_config(
    args: &BinaryEncodeArgs,
    payload: &[u8],
    payload_bits: usize,
    qr_size: usize,
) -> Result<EffectiveBinaryEncodeConfig, String> {
    if args.width.is_some() ^ args.height.is_some() {
        return Err(
            "--optimize requires both --width and --height when constraining dimensions".into(),
        );
    }

    let max_frames = args.n_frames.unwrap_or(DEFAULT_FRAMES);

    if let (Some(width), Some(height)) = (args.width, args.height) {
        validate_binary_capacity(width, height, payload_bits, qr_size)?;
        for n_frames in 1..=max_frames {
            if binary_roundtrip_succeeds(
                &args.qr_key,
                payload,
                width,
                height,
                n_frames,
                &args.seed,
                args.base_bias,
                args.payload_bias_delta,
            )? {
                return Ok(EffectiveBinaryEncodeConfig {
                    width,
                    height,
                    n_frames,
                });
            }
        }

        return Err(
            "could not find a viable frame count within the requested optimization bounds".into(),
        );
    }

    let min_dim = qr_size.max(ceil_sqrt(payload_bits.max(1)));
    let max_dim = DEFAULT_WIDTH.min(DEFAULT_HEIGHT);
    if min_dim > max_dim {
        return Err(format!(
            "payload requires at least {min_dim}x{min_dim} cells, which exceeds the supported optimized search space of {max_dim}x{max_dim}; current binary codec needs at least one cell per payload bit in a single window"
        ));
    }

    for dim in min_dim..=max_dim {
        for n_frames in 1..=max_frames {
            if binary_roundtrip_succeeds(
                &args.qr_key,
                payload,
                dim,
                dim,
                n_frames,
                &args.seed,
                args.base_bias,
                args.payload_bias_delta,
            )? {
                return Ok(EffectiveBinaryEncodeConfig {
                    width: dim,
                    height: dim,
                    n_frames,
                });
            }
        }
    }

    Err("could not find an optimized binary configuration within the supported search space".into())
}

#[allow(clippy::too_many_arguments)]
fn binary_roundtrip_succeeds(
    qr_key: &str,
    payload: &[u8],
    width: usize,
    height: usize,
    n_frames: usize,
    seed: &str,
    base_bias: f32,
    payload_bias_delta: f32,
) -> Result<bool, String> {
    let encoder = BinaryEncoder::new(
        n_frames,
        (width, height),
        seed.to_string(),
        base_bias,
        payload_bias_delta,
    )
    .map_err(|err| format!("failed to construct binary encoder during optimization: {err}"))?;
    let frames = encoder
        .encode_message(qr_key, payload)
        .map_err(|err| format!("failed to encode payload during optimization: {err}"))?;
    let decoder = BinaryDecoder::new(payload.len(), base_bias)
        .map_err(|err| format!("failed to construct binary decoder during optimization: {err}"))?;
    let result = decoder
        .decode_message(&frames)
        .map_err(|err| format!("failed to decode payload during optimization: {err}"))?;

    Ok(result.message.as_deref() == Some(qr_key) && result.payload.as_deref() == Some(payload))
}

fn validate_binary_capacity(
    width: usize,
    height: usize,
    payload_bits: usize,
    qr_size: usize,
) -> Result<(), String> {
    if width < qr_size || height < qr_size {
        return Err(format!(
            "frame size {}x{} is smaller than the QR required for this key ({}x{})",
            width, height, qr_size, qr_size
        ));
    }

    let n_cells = width
        .checked_mul(height)
        .ok_or_else(|| "frame dimensions overflow usize".to_string())?;
    if payload_bits > n_cells {
        return Err(format!(
            "payload requires {payload_bits} bit positions but frame only has {n_cells} cells; current binary codec needs at least one cell per payload bit in a single window"
        ));
    }

    Ok(())
}

fn ceil_sqrt(value: usize) -> usize {
    let mut n = 0usize;
    while n.saturating_mul(n) < value {
        n += 1;
    }
    n
}

#[cfg(test)]
mod tests {
    use super::{
        BinaryDecodeArgs, BinaryEncodeArgs, DEFAULT_BASE_BIAS, DEFAULT_PAYLOAD_BIAS_DELTA,
        DEFAULT_SEED, resolve_binary_encode_config,
    };
    use std::path::PathBuf;

    #[test]
    fn parse_binary_encode_args_with_text_payload() {
        let args = BinaryEncodeArgs::parse(
            [
                "--qr-key",
                "hello-key",
                "--payload-text",
                "Hello World",
                "--out",
                "hello.qrsb",
            ]
            .into_iter()
            .map(str::to_string),
        )
        .unwrap();

        assert_eq!(args.qr_key, "hello-key");
        assert_eq!(args.payload_text.as_deref(), Some("Hello World"));
        assert_eq!(args.payload_file, None);
        assert_eq!(args.out, PathBuf::from("hello.qrsb"));
        assert_eq!(args.width, None);
        assert_eq!(args.height, None);
        assert_eq!(args.n_frames, None);
        assert_eq!(args.seed, DEFAULT_SEED);
        assert_eq!(args.base_bias, DEFAULT_BASE_BIAS);
        assert_eq!(args.payload_bias_delta, DEFAULT_PAYLOAD_BIAS_DELTA);
        assert!(!args.optimize);
    }

    #[test]
    fn optimize_finds_smaller_viable_hello_world_config() {
        let args = BinaryEncodeArgs::parse(
            [
                "--qr-key",
                "hello-key",
                "--payload-text",
                "Hello World",
                "--optimize",
                "--out",
                "hello.qrsb",
            ]
            .into_iter()
            .map(str::to_string),
        )
        .unwrap();
        let payload = args.read_payload().unwrap();
        let effective = resolve_binary_encode_config(&args, &payload).unwrap();
        assert_eq!(effective.width, 25);
        assert_eq!(effective.height, 25);
        assert!(effective.n_frames <= 20);
        assert!(effective.n_frames >= 4);
    }

    #[test]
    fn oversized_payload_is_rejected_by_capacity_check() {
        let args = BinaryEncodeArgs::parse(
            [
                "--qr-key",
                "smiley-key",
                "--payload-text",
                &"A".repeat(300),
                "--out",
                "smiley.qrsb",
            ]
            .into_iter()
            .map(str::to_string),
        )
        .unwrap();
        let payload = args.read_payload().unwrap();
        let err = resolve_binary_encode_config(&args, &payload).unwrap_err();
        assert!(err.contains("current binary codec needs at least one cell per payload bit"));
    }

    #[test]
    fn parse_binary_decode_args() {
        let args = BinaryDecodeArgs::parse(
            ["--in", "hello.qrsb", "--payload-out", "hello.bin"]
                .into_iter()
                .map(str::to_string),
        )
        .unwrap();
        assert_eq!(args.input, PathBuf::from("hello.qrsb"));
        assert_eq!(args.payload_out, Some(PathBuf::from("hello.bin")));
    }
}
