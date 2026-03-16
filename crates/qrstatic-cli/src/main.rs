use std::env;
use std::process::ExitCode;

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
        Some(codec) => {
            println!("encode requested for codec '{codec}'");
            println!("codec plumbing is not implemented yet");
            ExitCode::SUCCESS
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
        Some(codec) => {
            println!("decode requested for codec '{codec}'");
            println!("codec plumbing is not implemented yet");
            ExitCode::SUCCESS
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
}
