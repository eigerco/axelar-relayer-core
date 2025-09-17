use clap::{Parser, Subcommand};
use xshell::{Shell, cmd};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Deny {
        #[clap(last = true)]
        args: Vec<String>,
    },
    Test {
        #[clap(short, long, default_value_t = false)]
        coverage: bool,
        #[clap(last = true)]
        args: Vec<String>,
    },
    Check,
    Fmt,
    Doc,
    UnusedDeps,
}

fn main() -> eyre::Result<()> {
    color_eyre::install()?;
    let sh = Shell::new()?;
    let args = Args::parse();

    match args.command {
        Commands::Deny { args } => {
            println!("cargo deny");
            cmd!(sh, "cargo install --version 0.17.0 cargo-deny").run()?;
            cmd!(sh, "cargo deny check {args...}").run()?;
        }
        Commands::Test { args, coverage } => {
            println!("cargo test");
            cmd!(sh, "cargo install cargo-nextest").run()?;

            // Doc tests
            cmd!(sh, "cargo test --doc -p retry").run()?;
            cmd!(sh, "cargo test --doc -p amplifier-api").run()?;
            cmd!(sh, "cargo test --doc -p infrastructure").run()?;
            cmd!(sh, "cargo test --doc -p bin-util").run()?;
            cmd!(sh, "cargo test --doc -p common-serde-utils").run()?;

            // Doc tests for mutually exclusive features
            cmd!(
                sh,
                "cargo test --doc -p amplifier-subscriber --no-default-features --features=nats"
            )
            .run()?;
            cmd!(
                sh,
                "cargo test --doc -p amplifier-ingester --no-default-features --features=nats"
            )
            .run()?;
            cmd!(
                sh,
                "cargo test --doc -p amplifier-subscriber --features=gcp"
            )
            .run()?;
            cmd!(sh, "cargo test --doc -p amplifier-ingester --features=gcp").run()?;

            if coverage {
                cmd!(sh, "cargo install grcov").run()?;
                for (key, val) in [
                    ("CARGO_INCREMENTAL", "0"),
                    ("RUSTFLAGS", "-Cinstrument-coverage"),
                    ("LLVM_PROFILE_FILE", "target/coverage/%p-%m.profraw"),
                ] {
                    sh.set_var(key, val);
                }
            }

            let args = &args;

            // Test basic crates
            cmd!(
                sh,
                "cargo nextest run -p retry --tests --all-targets --no-fail-fast {args...}"
            )
            .run()?;
            cmd!(
                sh,
                "cargo nextest run -p bin-util --tests --all-targets --no-fail-fast {args...}"
            )
            .run()?;

            cmd!(
                sh,
                "cargo nextest run -p amplifier-api --tests --all-targets --no-fail-fast {args...}"
            )
            .run()?;

            if coverage {
                cmd!(sh, "mkdir -p target/coverage").run()?;
                cmd!(sh, "grcov . --binary-path ./target/debug/deps/ -s . -t html,cobertura --branch --ignore-not-existing --ignore '../*' --ignore \"/*\" -o target/coverage/").run()?;

                // Open the generated file
                if std::option_env!("CI").is_none() {
                    #[cfg(target_os = "macos")]
                    cmd!(sh, "open target/coverage/html/index.html").run()?;

                    #[cfg(target_os = "linux")]
                    cmd!(sh, "xdg-open target/coverage/html/index.html").run()?;
                }
            }
        }

        Commands::Check => {
            println!("cargo check");

            // Basic crates
            cmd!(sh, "cargo clippy -p retry --locked -- -D warnings").run()?;
            cmd!(
                sh,
                "cargo clippy -p common-serde-utils --locked -- -D warnings"
            )
            .run()?;
            cmd!(sh, "cargo clippy -p bin-util --locked -- -D warnings").run()?;

            cmd!(sh, "cargo clippy -p amplifier-api --locked -- -D warnings").run()?;

            // Check infrastructure with mutually exclusive backends
            cmd!(
                sh,
                "cargo clippy -p infrastructure --features=gcp --locked -- -D warnings"
            )
            .run()?;
            cmd!(sh, "cargo clippy -p infrastructure --no-default-features --features=nats --locked -- -D warnings").run()?;

            // Check ingester with different backends
            cmd!(
                sh,
                "cargo clippy -p amplifier-ingester --features=gcp --locked -- -D warnings"
            )
            .run()?;
            cmd!(sh, "cargo clippy -p amplifier-ingester --no-default-features --features=nats --locked -- -D warnings").run()?;

            // Check subscriber with different backends
            cmd!(
                sh,
                "cargo clippy -p amplifier-subscriber --features=gcp --locked -- -D warnings"
            )
            .run()?;
            cmd!(sh, "cargo clippy -p amplifier-subscriber --no-default-features --features=nats --locked -- -D warnings").run()?;

            cmd!(sh, "cargo fmt --all --check").run()?;
        }
        Commands::Fmt => {
            println!("cargo fix");
            cmd!(sh, "cargo fmt --all").run()?;
            cmd!(
                sh,
                "cargo fix --allow-dirty --allow-staged --workspace --all-features --tests"
            )
            .run()?;
            cmd!(
                sh,
                "cargo clippy --fix --allow-dirty --allow-staged --workspace --all-features --tests"
            )
            .run()?;
        }
        Commands::Doc => {
            println!("cargo doc");
            cmd!(
                sh,
                "cargo doc --workspace --no-deps --no-default-features --features=nats"
            )
            .run()?;
            cmd!(sh, "cargo doc --workspace --no-deps --features=gcp").run()?;

            if std::option_env!("CI").is_none() {
                #[cfg(target_os = "macos")]
                {
                    cmd!(sh, "open target/doc/amplifier_api/index.html").run()?;
                    cmd!(sh, "open target/doc/amplifier_ingester/index.html").run()?;
                    cmd!(sh, "open target/doc/amplifier_subscriber/index.html").run()?;
                    cmd!(sh, "open target/doc/bin_util/index.html").run()?;
                    cmd!(sh, "open target/doc/retry/index.html").run()?;
                    cmd!(sh, "open target/doc/infrastructure/index.html").run()?;
                }

                #[cfg(target_os = "linux")]
                {
                    cmd!(sh, "xdg-open target/doc/amplifier_api/index.html").run()?;
                    cmd!(sh, "xdg-open target/doc/amplifier_ingester/index.html").run()?;
                    cmd!(sh, "xdg-open target/doc/amplifier_subscriber/index.html").run()?;
                    cmd!(sh, "xdg-open target/doc/bin_util/index.html").run()?;
                    cmd!(sh, "xdg-open target/doc/retry/index.html").run()?;
                    cmd!(sh, "xdg-open target/doc/infrastructure/index.html").run()?;
                }
            }
        }
        Commands::UnusedDeps => {
            println!("unused deps");
            cmd!(sh, "cargo install --version 0.7.0 cargo-machete").run()?;
            cmd!(sh, "cargo-machete").run()?;
        }
    }

    Ok(())
}
