//! Development harness for the core engine.
//!
//! The shipped product is a tray application; this binary exists so each provider can be
//! verified against real logs from a terminal before any UI exists.

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = args.first().map(String::as_str);

    match command {
        Some("list") => list(),
        Some("debug") => match args.get(1) {
            Some(key) => debug_provider(key),
            None => {
                eprintln!("debug requires a provider key; run `quotadeck list` to see them");
                ExitCode::FAILURE
            }
        },
        Some(other) => {
            eprintln!("unknown command: {other}");
            usage();
            ExitCode::FAILURE
        }
        None => {
            usage();
            ExitCode::FAILURE
        }
    }
}

fn usage() {
    eprintln!(
        "usage:\n  quotadeck list            detected providers on this machine\n  quotadeck debug <key>     parse one provider and print what it found"
    );
}

fn list() -> ExitCode {
    let providers = quotadeck_providers::all();
    if providers.is_empty() {
        println!("no providers are registered in this build yet");
        return ExitCode::SUCCESS;
    }

    for provider in providers {
        let roots = provider.discover_roots();
        let status = if roots.is_empty() {
            "not installed".to_string()
        } else {
            roots
                .iter()
                .map(|root| root.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        };
        let level = if provider.supports_measured() {
            "measured"
        } else {
            "derived"
        };
        println!("{:<14} {:<9} {}", provider.id().key(), level, status);
    }
    ExitCode::SUCCESS
}

fn debug_provider(key: &str) -> ExitCode {
    let Some(provider) = quotadeck_providers::by_key(key) else {
        eprintln!("unknown provider: {key}");
        return ExitCode::FAILURE;
    };

    let roots = provider.discover_roots();
    if roots.is_empty() {
        println!(
            "{} is not installed on this machine",
            provider.display_name()
        );
        return ExitCode::SUCCESS;
    }

    println!("{} roots:", provider.display_name());
    for root in &roots {
        println!("  {}", root.display());
    }
    println!("globs: {:?}", provider.watch_globs());
    ExitCode::SUCCESS
}
