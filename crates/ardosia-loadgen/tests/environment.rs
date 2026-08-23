use ardosia_loadgen::environment::collect_environment;
use ardosia_loadgen::report::EnvironmentReport;

const UPSTREAM_REVISION: &str = "3edfb4170e6cb5aeed992b09b50176fb7e5b6079";
const HARDFORK_REVISION: &str = "f127fce27a206a51a1d39ffa7a9bbed98d10ea14";

#[test]
fn environment_revision_metadata_is_explicit_and_legacy_compatible() {
    let value = serde_json::to_value(EnvironmentReport::default()).unwrap();

    assert_eq!(
        value
            .get("raknet_upstream_revision")
            .and_then(serde_json::Value::as_str),
        Some(UPSTREAM_REVISION)
    );
    assert_eq!(
        value
            .get("raknet_hardfork_revision")
            .and_then(serde_json::Value::as_str),
        Some(HARDFORK_REVISION)
    );
    assert!(value.get("vendor_revision").is_none());

    let legacy: EnvironmentReport = serde_json::from_value(serde_json::json!({
        "git_commit": null,
        "vendor_revision": "legacy-upstream-revision",
        "rust_version": null,
        "os": "linux",
        "kernel": null,
        "architecture": "x86_64",
        "logical_cpus": 1,
        "total_memory_bytes": null,
        "build_profile": "debug"
    }))
    .unwrap();
    let roundtrip = serde_json::to_value(legacy).unwrap();

    assert_eq!(
        roundtrip
            .get("raknet_upstream_revision")
            .and_then(serde_json::Value::as_str),
        Some("legacy-upstream-revision")
    );
    assert!(roundtrip.get("vendor_revision").is_none());
}

#[test]
fn hardfork_revision_metadata_matches_workspace_pin() {
    let value = serde_json::to_value(EnvironmentReport::default()).unwrap();
    let reported_revision = value
        .get("raknet_hardfork_revision")
        .and_then(serde_json::Value::as_str)
        .expect("environment report must include the active RakNet hardfork revision");
    let workspace_manifest = include_str!("../../../Cargo.toml");

    assert_eq!(reported_revision, HARDFORK_REVISION);
    assert!(workspace_manifest.contains(&format!("rev = \"{reported_revision}\"")));
}

#[cfg(target_os = "linux")]
#[test]
fn linux_environment_records_open_file_limits() {
    let value = serde_json::to_value(collect_environment()).unwrap();
    let open_files = &value["process_limits"]["open_files"];

    assert!(
        !open_files.is_null(),
        "Linux environment report must expose the load-generator open-file limit"
    );
    assert!(
        open_files.get("soft").is_some(),
        "open-file report must distinguish the soft bound"
    );
    assert!(
        open_files.get("hard").is_some(),
        "open-file report must distinguish the hard bound"
    );
}
