use ardosia_loadgen::cli::{Cli, Command};
use clap::Parser;

#[test]
fn parses_profile_command_without_manual_pid() {
    let cli = Cli::try_parse_from([
        "ardosia-loadgen",
        "profile",
        "scenarios/steady-1000.toml",
        "--output",
        "profiles/steady-1000",
    ])
    .unwrap();

    match cli.command {
        Command::Profile {
            scenario, output, ..
        } => {
            assert_eq!(scenario.to_string_lossy(), "scenarios/steady-1000.toml");
            assert_eq!(
                output.unwrap().to_string_lossy(),
                "profiles/steady-1000"
            );
        }
        other => panic!("expected profile command, got {other:?}"),
    }
}
