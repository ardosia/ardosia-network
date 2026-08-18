use ardosia_loadgen::frame::{BenchmarkFrame, FRAME_MAGIC, FRAME_VERSION, FrameKind};
use bytes::Bytes;

fn roundtrip(kind: FrameKind, payload: Bytes) {
    let frame = BenchmarkFrame {
        kind,
        client_id: 42,
        sequence: 99,
        probe_id: 123,
        payload,
    };
    let encoded = frame.encode();
    let decoded = BenchmarkFrame::decode(&encoded).unwrap();
    assert_eq!(decoded, frame);
}

#[test]
fn every_frame_kind_roundtrips() {
    for kind in [
        FrameKind::UnreliableData,
        FrameKind::ReliableOrderedData,
        FrameKind::FragmentedReliableOrderedData,
        FrameKind::EchoRequest,
        FrameKind::EchoResponse,
    ] {
        roundtrip(kind, Bytes::from_static(b"payload"));
    }
}

#[test]
fn empty_and_fragment_sized_payloads_roundtrip() {
    roundtrip(FrameKind::ReliableOrderedData, Bytes::new());
    roundtrip(
        FrameKind::FragmentedReliableOrderedData,
        Bytes::from(vec![0x5a; 4096]),
    );
}

#[test]
fn malformed_headers_are_rejected_without_panicking() {
    assert!(BenchmarkFrame::decode(&[]).is_err());
    assert!(BenchmarkFrame::decode(&[0; 8]).is_err());

    let good = BenchmarkFrame {
        kind: FrameKind::EchoRequest,
        client_id: 1,
        sequence: 2,
        probe_id: 3,
        payload: Bytes::new(),
    }
    .encode();

    let mut wrong_magic = good.to_vec();
    wrong_magic[..4].copy_from_slice(b"NOPE");
    assert!(BenchmarkFrame::decode(&wrong_magic).is_err());

    let mut wrong_version = good.to_vec();
    wrong_version[FRAME_MAGIC.len()] = FRAME_VERSION.wrapping_add(1);
    assert!(BenchmarkFrame::decode(&wrong_version).is_err());

    let mut unknown_kind = good.to_vec();
    unknown_kind[FRAME_MAGIC.len() + 1] = 0xff;
    assert!(BenchmarkFrame::decode(&unknown_kind).is_err());
}
