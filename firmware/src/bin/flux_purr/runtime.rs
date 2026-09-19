// Runtime assembly is kept in responsibility-oriented modules. The explicit
// re-exports preserve the existing private runtime namespace for the binary.
#[path = "adc.rs"]
pub(crate) mod adc;
#[path = "boot.rs"]
pub(crate) mod boot;
#[path = "control_plane.rs"]
pub(crate) mod control_plane;
#[path = "display_io.rs"]
pub(crate) mod display_io;
#[path = "eeprom.rs"]
pub(crate) mod eeprom;
#[path = "eeprom_snapshot.rs"]
pub(crate) mod eeprom_snapshot;
#[path = "fan.rs"]
pub(crate) mod fan;
#[path = "frontpanel.rs"]
pub(crate) mod frontpanel;
#[path = "lan.rs"]
pub(crate) mod lan;
#[path = "pd_control.rs"]
pub(crate) mod pd_control;
#[path = "pd_protocol.rs"]
pub(crate) mod pd_protocol;
#[path = "pd_service.rs"]
pub(crate) mod pd_service;
#[path = "power.rs"]
pub(crate) mod power;
#[path = "runtime_loop.rs"]
pub(crate) mod runtime_loop;
#[path = "support.rs"]
pub(crate) mod support;
#[path = "tasks.rs"]
pub(crate) mod tasks;
#[path = "thermal.rs"]
pub(crate) mod thermal;
#[path = "watchdog.rs"]
pub(crate) mod watchdog;

#[allow(unused_imports)]
pub(crate) use adc::*;
#[allow(unused_imports)]
pub(crate) use boot::*;
#[allow(unused_imports)]
pub(crate) use control_plane::*;
#[allow(unused_imports)]
pub(crate) use display_io::*;
#[allow(unused_imports)]
pub(crate) use eeprom::*;
#[allow(unused_imports)]
pub(crate) use eeprom_snapshot::*;
#[allow(unused_imports)]
pub(crate) use fan::*;
#[allow(unused_imports)]
pub(crate) use frontpanel::*;
#[allow(unused_imports)]
pub(crate) use lan::*;
#[allow(unused_imports)]
pub(crate) use pd_control::*;
#[allow(unused_imports)]
pub(crate) use pd_protocol::*;
#[allow(unused_imports)]
pub(crate) use pd_service::*;
#[allow(unused_imports)]
pub(crate) use power::*;
#[allow(unused_imports)]
pub(crate) use runtime_loop::*;
#[allow(unused_imports)]
pub(crate) use support::*;
#[allow(unused_imports)]
pub(crate) use tasks::*;
#[allow(unused_imports)]
pub(crate) use thermal::*;
#[allow(unused_imports)]
pub(crate) use watchdog::*;

#[cfg(test)]
#[path = "runtime_tests.rs"]
mod tests;
