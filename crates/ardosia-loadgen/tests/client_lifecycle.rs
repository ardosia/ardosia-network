use ardosia_loadgen::churn::{
    DisconnectIntent, DisconnectOutcome, classify_disconnect,
};

#[test]
fn planned_clean_disconnect_is_not_final_clean_or_unexpected() {
    let counts = classify_disconnect(DisconnectIntent::PlannedChurn, DisconnectOutcome::Clean);
    assert_eq!(counts.completed_planned_disconnects, 1);
    assert_eq!(counts.clean_disconnects, 0);
    assert_eq!(counts.unexpected_disconnects, 0);
}

#[test]
fn final_clean_disconnect_keeps_existing_semantics() {
    let counts = classify_disconnect(DisconnectIntent::FinalShutdown, DisconnectOutcome::Clean);
    assert_eq!(counts.completed_planned_disconnects, 0);
    assert_eq!(counts.clean_disconnects, 1);
    assert_eq!(counts.unexpected_disconnects, 0);
}

#[test]
fn failed_disconnect_is_unexpected_for_both_intents() {
    for intent in [DisconnectIntent::PlannedChurn, DisconnectIntent::FinalShutdown] {
        let counts = classify_disconnect(intent, DisconnectOutcome::Failed);
        assert_eq!(counts.completed_planned_disconnects, 0);
        assert_eq!(counts.clean_disconnects, 0);
        assert_eq!(counts.unexpected_disconnects, 1);
    }
}
