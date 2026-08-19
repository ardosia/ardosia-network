use std::path::Path;
use std::str::FromStr;

use ardosia_loadgen::child_protocol::run_stdio_child;
use ardosia_loadgen::cli::{Cli, Command};
use ardosia_loadgen::report::RunReport;
use ardosia_loadgen::resource::ResourceSummary;
use ardosia_loadgen::runner::{run_clients, run_local, run_profile, serve_until};
use ardosia_loadgen::scenario::Scenario;
use clap::Parser;
use tokio::sync::watch;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Command::Local { scenario, bind } => {
            let scenario = load_scenario(&scenario)?;
            let report = run_local(bind, &scenario).await?;
            emit_report(&report)?;
        }
        Command::Profile {
            scenario,
            bind,
            output,
        } => {
            let loaded = load_scenario(&scenario)?;
            let profile = run_profile(bind, &scenario, &loaded, output.as_deref()).await?;
            eprintln!("profile: {}", profile.output_dir.display());
            emit_report(&profile.report)?;
        }
        Command::Run { scenario, target } => {
            let scenario = load_scenario(&scenario)?;
            let report = run_clients(target, &scenario).await;
            emit_report(&report)?;
        }
        Command::Serve {
            bind,
            protocol,
            max_connections,
        } => {
            let (stop_tx, stop_rx) = watch::channel(false);
            tokio::spawn(async move {
                let _ = tokio::signal::ctrl_c().await;
                let _ = stop_tx.send(true);
            });

            let metrics = serve_until(bind, protocol, max_connections, stop_rx).await?;
            eprintln!("server stopped: {metrics:?}");
        }
        Command::ServeChild => run_stdio_child().await?,
    }

    Ok(())
}

fn load_scenario(path: &Path) -> Result<Scenario, Box<dyn std::error::Error>> {
    let input = std::fs::read_to_string(path)?;
    Ok(Scenario::from_str(&input)?)
}

fn emit_report(report: &RunReport) -> Result<(), Box<dyn std::error::Error>> {
    print_summary(report);
    println!("{}", serde_json::to_string_pretty(report)?);
    if !report.results.passed {
        std::process::exit(1);
    }
    Ok(())
}

fn print_summary(report: &RunReport) {
    let results = &report.results;
    let counts = results.correctness;
    let transport = results.transport.delta;
    let queue_failures = transport
        .outgoing_queue_drops
        .saturating_add(transport.outgoing_queue_disconnects)
        .saturating_add(transport.backpressure_drops)
        .saturating_add(transport.backpressure_disconnects);

    eprintln!(
        "sessions: {}/{}",
        counts.successful_handshakes, report.scenario.clients
    );
    eprintln!(
        "errors: disconnect={} protocol={} send={} backpressure_drop={}",
        counts.unexpected_disconnects, counts.protocol_errors, counts.send_errors, queue_failures
    );
    eprintln!(
        "traffic: tx={:.1} pkt/s rx={:.1} pkt/s tx={:.1} KiB/s rx={:.1} KiB/s",
        results.workload.tx_frames_per_second,
        results.workload.rx_frames_per_second,
        results.workload.tx_payload_bytes_per_second / 1024.0,
        results.workload.rx_payload_bytes_per_second / 1024.0,
    );
    eprintln!(
        "rtt: samples={} p50={} p95={} p99={} max={}",
        results.latency.samples,
        fmt_ms(results.latency.p50_ms),
        fmt_ms(results.latency.p95_ms),
        fmt_ms(results.latency.p99_ms),
        fmt_ms(results.latency.max_ms),
    );
    eprintln!(
        "raknet: retransmits={} ack={} nack={} queue_peak={} bytes",
        transport.retransmitted_datagrams,
        transport.acks_out,
        transport.nacks_out,
        results.transport.peaks.pending_outgoing_bytes_peak,
    );
    print_process("server", results.resources.steady_server.as_ref());
    print_process("loadgen", results.resources.steady_loadgen.as_ref());
    print_host(results.resources.steady_host.as_ref());
    eprintln!("result: {}", if results.passed { "PASS" } else { "FAIL" });
    for reason in &results.failure_reasons {
        eprintln!("failure: {reason}");
    }
}

fn print_process(name: &str, summary: Option<&ResourceSummary>) {
    let summary = summary.cloned().unwrap_or_default();
    eprintln!(
        "{name}: cpu_avg={} cpu_peak={} rss_peak={}",
        fmt_pct(summary.process_cpu_avg_pct),
        fmt_pct(summary.process_cpu_peak_pct),
        fmt_bytes(summary.process_rss_peak_bytes),
    );
}

fn print_host(summary: Option<&ResourceSummary>) {
    let summary = summary.cloned().unwrap_or_default();
    eprintln!(
        "host: cpu_avg={} cpu_peak={} memory_peak={} memory_available_min={}",
        fmt_pct(summary.host_cpu_avg_pct),
        fmt_pct(summary.host_cpu_peak_pct),
        fmt_bytes(summary.host_memory_used_peak_bytes),
        fmt_bytes(summary.host_memory_available_min_bytes),
    );
}

fn fmt_ms(value: Option<f64>) -> String {
    value.map_or_else(|| "n/a".into(), |value| format!("{value:.1}ms"))
}

fn fmt_pct(value: Option<f64>) -> String {
    value.map_or_else(|| "n/a".into(), |value| format!("{value:.1}%"))
}

fn fmt_bytes(value: Option<u64>) -> String {
    value.map_or_else(
        || "n/a".into(),
        |value| format!("{:.1} MiB", value as f64 / (1024.0 * 1024.0)),
    )
}
