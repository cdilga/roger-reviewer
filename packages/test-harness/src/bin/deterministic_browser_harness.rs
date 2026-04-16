use roger_test_harness::{
    BrowserHarnessConfig, BrowserHarnessOutcome, BrowserHarnessRuntime, run_browser_harness,
};
use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

fn usage() -> &'static str {
    "usage: deterministic_browser_harness --browser-binary <path> --extension-dir <path> --artifact-root <path> [--start-url <url>] [--runtime deterministic_chromium|chrome_smoke|brave_smoke|edge_smoke] [--startup-probe-ms <ms>]"
}

fn main() -> ExitCode {
    match parse_args(env::args().skip(1).collect()) {
        Ok(config) => match run_browser_harness(&config) {
            Ok(report) => {
                match serde_json::to_string_pretty(&report) {
                    Ok(json) => println!("{json}"),
                    Err(err) => {
                        eprintln!("failed to serialize browser harness report: {err}");
                        return ExitCode::from(1);
                    }
                }
                match report.outcome {
                    BrowserHarnessOutcome::Launched => ExitCode::SUCCESS,
                    BrowserHarnessOutcome::Blocked => ExitCode::from(3),
                }
            }
            Err(err) => {
                eprintln!("browser harness failed: {err}");
                ExitCode::from(1)
            }
        },
        Err(err) => {
            eprintln!("{err}");
            eprintln!("{}", usage());
            ExitCode::from(2)
        }
    }
}

fn parse_args(args: Vec<String>) -> Result<BrowserHarnessConfig, String> {
    let mut browser_binary = None;
    let mut extension_dir = None;
    let mut artifact_root = None;
    let mut start_url = "https://github.com/owner/repo/pull/1".to_owned();
    let mut runtime = BrowserHarnessRuntime::DeterministicChromium;
    let mut startup_probe_ms = 200;

    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--browser-binary" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "--browser-binary requires a value".to_owned())?;
                browser_binary = Some(PathBuf::from(value));
                index += 2;
            }
            "--extension-dir" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "--extension-dir requires a value".to_owned())?;
                extension_dir = Some(PathBuf::from(value));
                index += 2;
            }
            "--artifact-root" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "--artifact-root requires a value".to_owned())?;
                artifact_root = Some(PathBuf::from(value));
                index += 2;
            }
            "--start-url" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "--start-url requires a value".to_owned())?;
                start_url = value.clone();
                index += 2;
            }
            "--runtime" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "--runtime requires a value".to_owned())?;
                runtime = BrowserHarnessRuntime::parse(value)?;
                index += 2;
            }
            "--startup-probe-ms" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "--startup-probe-ms requires a value".to_owned())?;
                startup_probe_ms = value
                    .parse::<u64>()
                    .map_err(|_| "--startup-probe-ms must be an integer".to_owned())?;
                index += 2;
            }
            "-h" | "--help" => {
                return Err("help requested".to_owned());
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }

    Ok(BrowserHarnessConfig {
        browser_binary: browser_binary.ok_or_else(|| "--browser-binary is required".to_owned())?,
        extension_dir: extension_dir.ok_or_else(|| "--extension-dir is required".to_owned())?,
        artifact_root: artifact_root.ok_or_else(|| "--artifact-root is required".to_owned())?,
        start_url,
        runtime,
        startup_probe_ms,
    })
}
