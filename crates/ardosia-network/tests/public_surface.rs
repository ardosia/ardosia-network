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
