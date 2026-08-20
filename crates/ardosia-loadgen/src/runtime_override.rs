use std::sync::OnceLock;

static WORKER_SHARDS: OnceLock<Option<usize>> = OnceLock::new();

pub fn configure_worker_shards(worker_shards: Option<usize>) {
    let _ = WORKER_SHARDS.set(worker_shards);
}

pub(crate) fn worker_shards() -> Option<usize> {
    WORKER_SHARDS.get().copied().flatten()
}

pub(crate) fn worker_shards_setting() -> Option<Option<usize>> {
    WORKER_SHARDS.get().copied()
}
