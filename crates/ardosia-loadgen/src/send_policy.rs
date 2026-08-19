use ardosia_network::NetworkError;

pub fn counts_as_benchmark_send_error(error: &NetworkError) -> bool {
    !matches!(error, NetworkError::ConnectionClosed)
}
