#[test]
fn crate_root_keeps_raknet_implementation_private() {
    let crate_root = include_str!("../src/lib.rs");

    assert!(
        !crate_root.contains("raknet_rust"),
        "crate root must not re-export or expose raknet-rust implementation types"
    );
    assert!(
        !crate_root
            .lines()
            .map(str::trim_start)
            .any(|line| line.starts_with("pub mod ")),
        "implementation modules must remain private; expose Ardosia-owned facade types explicitly"
    );
}
