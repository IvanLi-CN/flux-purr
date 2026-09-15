#![cfg_attr(target_arch = "xtensa", no_std)]
#![cfg_attr(target_arch = "xtensa", no_main)]

#[path = "flux_purr/runtime.rs"]
mod runtime;

#[cfg(target_arch = "xtensa")]
#[esp_rtos::main]
async fn main(spawner: embassy_executor::Spawner) {
    runtime::run(spawner).await;
}

#[cfg(not(target_arch = "xtensa"))]
fn main() {
    runtime::host_main();
}
