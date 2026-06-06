use wasmtime::{Config, Engine, OptLevel};
use anyhow::Result;

pub fn build_engine() -> Result<Engine> {
    let mut config = Config::new();
    config.target("pulley64")?;       // Pulley固定
    config.consume_fuel(true);         // fuel-basedタイムスライス
    config.wasm_exceptions(true);      // Exception Handling Proposal
    config.cranelift_opt_level(OptLevel::Speed); // LICM等の最適化
    Ok(Engine::new(&config)?)
}
