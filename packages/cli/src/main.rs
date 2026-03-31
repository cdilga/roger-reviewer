use roger_cli::{CliRuntime, run};

fn main() {
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let runtime = CliRuntime::from_env(cwd);
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = run(&args, &runtime);

    if !result.stdout.is_empty() {
        print!("{}", result.stdout);
    }
    if !result.stderr.is_empty() {
        eprint!("{}", result.stderr);
    }

    std::process::exit(result.exit_code);
}
