//! One bounded Wasmtime engine per process, with independent fuel and wall-clock interruption.
use std::sync::OnceLock;
use wasmtime::{Config, Engine, InstanceAllocationStrategy, PoolingAllocationConfig};

pub(crate) const TABLE_ELEMENTS: usize = 10_000;
pub(crate) const MEMORY_BYTES: usize = 1 << 20;

pub(crate) fn shared() -> Result<Engine, String> {
    static ENGINE: OnceLock<Result<Engine, String>> = OnceLock::new();
    ENGINE
        .get_or_init(|| {
            let mut pool = PoolingAllocationConfig::default();
            pool.total_component_instances(64)
                .max_core_instances_per_component(4)
                .total_core_instances(64)
                .total_memories(64)
                .total_tables(64)
                .max_memories_per_module(1)
                .max_tables_per_module(1)
                .max_memory_size(MEMORY_BYTES)
                .table_elements(TABLE_ELEMENTS);
            let mut config = Config::new();
            config
                .wasm_component_model(true)
                .consume_fuel(true)
                .epoch_interruption(true)
                .max_wasm_stack(256 * 1024)
                .memory_reservation(MEMORY_BYTES as u64)
                .memory_guard_size(64 * 1024)
                .allocation_strategy(InstanceAllocationStrategy::Pooling(pool));
            let engine = Engine::new(&config).map_err(|e| e.to_string())?;
            let ticker = engine.clone();
            std::thread::Builder::new()
                .name("wasm-deadline".into())
                .spawn(move || {
                    loop {
                        std::thread::sleep(std::time::Duration::from_millis(10));
                        ticker.increment_epoch();
                    }
                })
                .map_err(|_| "Cannot start Wasm deadline clock".to_string())?;
            Ok(engine)
        })
        .clone()
}
