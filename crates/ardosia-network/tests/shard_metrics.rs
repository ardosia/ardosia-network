use ardosia_network::{NetworkServer, NetworkShardMetrics};

#[test]
fn network_server_exposes_per_shard_metrics() {
    let _: fn(&NetworkServer) -> Vec<NetworkShardMetrics> = NetworkServer::shard_metrics;
}
