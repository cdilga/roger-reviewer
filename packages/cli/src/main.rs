use std::io::IsTerminal;
use std::path::PathBuf;

use roger_bridge::{
    BridgePreflight, BridgeResponse, NativeBridgeMessage, handle_bridge_intent,
    read_native_bridge_message, write_launch_progress, write_native_message, write_native_value,
};
use roger_cli::{CliRuntime, run};

fn is_native_host_wrapper_arg(arg: &str) -> bool {
    arg.starts_with("chrome-extension://")
        || arg.starts_with("--parent-window")
        || arg.starts_with("--native-host")
}

fn run_native_host_mode(runtime: &CliRuntime) -> i32 {
    let binary_path = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("rr"));

    let mut stdin = std::io::stdin().lock();
    let message = read_native_bridge_message(&mut stdin);

    let mut stdout = std::io::stdout().lock();
    let response = match message {
        Ok(NativeBridgeMessage::StatusProbe(probe)) => {
            // Companion-tier bounded readback: read-only, store-backed, and
            // empty when nothing truthful can be mirrored. StatusProbe stays a
            // single-frame reply — it runs under a tight (~4s) extension-side
            // budget, so it must not spend that budget on ack/progress frames.
            let reply = roger_cli::answer_bridge_status_probe(
                runtime,
                &probe.owner,
                &probe.repo,
                probe.pr_number,
            );
            if let Err(err) = write_native_value(&mut stdout, &reply) {
                eprintln!("native messaging write failed: {err}");
                return 1;
            }
            return 0;
        }
        Ok(NativeBridgeMessage::Launch(intent)) => {
            // Loud, incremental launch feedback (see LAUNCH_PROGRESS_SCHEMA):
            // emit an immediate ack the moment the launch message parses so the
            // extension knows the host is alive well before the (possibly slow)
            // gh-auth preflight and child `rr` spawn finish. A failed progress
            // write means the extension port is already gone; nothing more we do
            // here can be delivered, so exit non-zero.
            if let Err(err) = write_launch_progress(&mut stdout, "host_started") {
                eprintln!("native messaging progress write failed: {err}");
                return 1;
            }

            // Preflight (gh auth, binary, data dir) runs after the ack so the ack
            // is never delayed by it. When it passes, emit a second progress
            // frame before handing off to the launcher; when it fails we skip
            // preflight_ok and fall straight through to the existing failure
            // response (which handle_bridge_intent produces below).
            let preflight = BridgePreflight::check(&binary_path, &runtime.store_root);
            if preflight.is_ready() {
                if let Err(err) = write_launch_progress(&mut stdout, "preflight_ok") {
                    eprintln!("native messaging progress write failed: {err}");
                    return 1;
                }
            }

            handle_bridge_intent(&intent, &preflight, &binary_path)
        }
        Err(err) => BridgeResponse::failure(
            "native_messaging_host",
            "Invalid bridge request",
            &err.to_string(),
        ),
    };

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
