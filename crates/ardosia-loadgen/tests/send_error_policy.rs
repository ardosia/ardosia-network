use ardosia_loadgen::send_policy::counts_as_benchmark_send_error;
use ardosia_network::NetworkError;

#[test]
fn connection_closed_at_teardown_is_not_a_benchmark_send_error() {
    assert!(!counts_as_benchmark_send_error(
        &NetworkError::ConnectionClosed
    ));
    assert!(counts_as_benchmark_send_error(&NetworkError::Backpressure));
    assert!(counts_as_benchmark_send_error(&NetworkError::BackendStopped));
}
