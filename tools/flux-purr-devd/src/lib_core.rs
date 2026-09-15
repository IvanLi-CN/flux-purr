// The daemon facade assembles responsibility-oriented modules while preserving
// the existing crate API through deliberate crate-visible re-exports.
#[path = "developer_backup.rs"]
pub mod developer_backup;
#[path = "firmware_bundle.rs"]
pub mod firmware_bundle;
#[path = "lan.rs"]
pub mod lan;

#[path = "lib_core/contracts.rs"]
pub mod contracts;
#[path = "lib_core/control_plane.rs"]
pub mod control_plane;
#[path = "lib_core/discovery.rs"]
pub mod discovery;
#[path = "lib_core/espflash.rs"]
pub mod espflash;
#[path = "lib_core/events.rs"]
pub mod events;
#[path = "lib_core/firmware_update.rs"]
pub mod firmware_update;
#[path = "lib_core/flash.rs"]
pub mod flash;
#[path = "lib_core/foundation.rs"]
pub mod foundation;
#[path = "lib_core/http.rs"]
pub mod http;
#[path = "lib_core/serial.rs"]
pub mod serial;
#[path = "lib_core/wifi_thermal.rs"]
pub mod wifi_thermal;

pub use contracts::*;
pub use control_plane::*;
pub use discovery::*;
pub(crate) use espflash::*;
pub(crate) use events::*;
pub(crate) use firmware_update::*;
pub(crate) use flash::*;
pub use foundation::*;
pub(crate) use http::*;
pub use serial::acquire_serial_port_lock;
pub(crate) use serial::*;
pub(crate) use wifi_thermal::*;

#[cfg(test)]
#[path = "lib_core_tests.rs"]
mod tests;
