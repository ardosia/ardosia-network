use std::process::Command;

use crate::report::EnvironmentReport;

pub fn collect_environment() -> EnvironmentReport {
    let mut report = EnvironmentReport::default();
    report.git_commit = std::env::var("GITHUB_SHA")
        .ok()
        .filter(|value| !value.is_empty())
        .or_else(|| command_line("git", &["rev-parse", "HEAD"]));
    report.rust_version = command_line("rustc", &["--version"]);
    report.kernel = command_line("uname", &["-sr"]);

    #[cfg(target_os = "linux")]
    {
        report.total_memory_bytes = crate::resource::linux::read_meminfo()
            .map(|memory| memory.total_bytes);
    }

    report
}

fn command_line(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8(output.stdout).ok()?;
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}
