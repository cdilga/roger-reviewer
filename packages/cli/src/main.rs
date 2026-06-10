use std::io::IsTerminal;
use std::path::PathBuf;

use roger_bridge::{
    BridgePreflight, BridgeResponse, handle_bridge_intent, read_native_message,
    write_native_message,
};
use roger_cli::{CliRuntime, run};

fn is_native_host_wrapper_arg(arg: &str) -> bool {
    arg.starts_with("chrome-extension://")
        || arg.starts_with("--parent-window")
        || arg.starts_with("--native-host")
}

fn run_native_host_mode(runtime: &CliRuntime) -> i32 {
    let binary_path = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("rr"));
    let preflight = BridgePreflight::check(&binary_path, &runtime.store_root);

    let mut stdin = std::io::stdin().lock();
    let intent = read_native_message(&mut stdin);

    let response = match intent {
        Ok(intent) => handle_bridge_intent(&intent, &preflight, &binary_path),
        Err(err) => BridgeResponse::failure(
            "native_messaging_host",
            "Invalid bridge request",
            &err.to_string(),
        ),
    };

    let mut stdout = std::io::stdout().lock();
    if let Err(err) = write_native_message(&mut stdout, &response) {
        eprintln!("native messaging write failed: {err}");
        return 1;
    }

    if response.ok { 0 } else { 1 }
}

fn main() {
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let runtime = CliRuntime::from_env(cwd);
    let args: Vec<String> = std::env::args().skip(1).collect();
    let stdin_is_terminal = std::io::stdin().is_terminal();
    let native_host_wrapper_args = !args.is_empty()
        && args
            .iter()
            .all(|arg| is_native_host_wrapper_arg(arg.as_str()));

    if !stdin_is_terminal && (args.is_empty() || native_host_wrapper_args) {
        std::process::exit(run_native_host_mode(&runtime));
    }

    let result = run(&args, &runtime);

    if !result.stdout.is_empty() {
        print!("{}", result.stdout);
    }
    if !result.stderr.is_empty() {
        eprint!("{}", result.stderr);
    }

    std::process::exit(result.exit_code);
}
