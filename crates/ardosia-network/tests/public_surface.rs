#[test]
fn crate_root_does_not_directly_expose_raknet_dependency() {
    let crate_root = include_str!("../src/lib.rs");

    let exposes_raknet = crate_root
        .lines()
        .map(str::trim_start)
        .filter(|line| line.starts_with("pub "))
        .any(|line| line.contains("raknet_rust"));

    assert!(
        !exposes_raknet,
        "crate root must not directly re-export or expose raknet-rust implementation types"
    );
}

#[test]
fn crate_root_exposes_no_metrics_facade() {
    let crate_root = include_str!("../src/lib.rs");
    assert!(!crate_root.contains("NetworkMetrics"));
    assert!(!crate_root.contains("NetworkShardMetrics"));
    assert!(!crate_root.contains("TransportMetrics"));
}

#[test]
fn crate_root_contains_only_the_intended_module_exports() {
    let crate_root = include_str!("../src/lib.rs");
    for expected in [
        "CookieMode",
        "NetworkConfig",
        "NetworkConfigError",
        "Connection",
        "NetworkError",
        "Reliability",
        "NetworkServer",
    ] {
        assert!(crate_root.contains(expected), "missing {expected}");
    }
    assert!(!crate_root.contains("NetworkRuntimeConfig"));
    assert!(!crate_root.contains("Metrics"));
}
