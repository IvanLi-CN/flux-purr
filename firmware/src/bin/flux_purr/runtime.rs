// Runtime assembly is kept in responsibility-oriented modules. The explicit
// re-exports preserve the existing private runtime namespace for the binary.
#[path = "adc.rs"]
#[allow(dead_code)]
pub(crate) mod adc;
#[path = "boot.rs"]
#[allow(dead_code)]
pub(crate) mod boot;
#[path = "control_plane.rs"]
#[allow(dead_code)]
pub(crate) mod control_plane;
#[path = "display_io.rs"]
#[allow(dead_code)]
pub(crate) mod display_io;
#[path = "eeprom.rs"]
#[allow(dead_code)]
pub(crate) mod eeprom;
#[path = "eeprom_snapshot.rs"]
#[allow(dead_code)]
pub(crate) mod eeprom_snapshot;
#[path = "fan.rs"]
#[allow(dead_code)]
pub(crate) mod fan;
#[path = "frontpanel.rs"]
#[allow(dead_code)]
pub(crate) mod frontpanel;
#[path = "lan.rs"]
#[allow(dead_code)]
pub(crate) mod lan;
#[path = "pd_control.rs"]
#[allow(dead_code)]
pub(crate) mod pd_control;
#[path = "pd_protocol.rs"]
#[allow(dead_code)]
pub(crate) mod pd_protocol;
#[path = "power.rs"]
#[allow(dead_code)]
pub(crate) mod power;
#[path = "runtime_loop.rs"]
#[allow(dead_code)]
pub(crate) mod runtime_loop;
#[path = "support.rs"]
#[allow(dead_code)]
pub(crate) mod support;
#[path = "tasks.rs"]
#[allow(dead_code)]
pub(crate) mod tasks;
#[path = "thermal.rs"]
#[allow(dead_code)]
pub(crate) mod thermal;

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
pub(crate) use power::*;
#[allow(unused_imports)]
pub(crate) use runtime_loop::*;
#[allow(unused_imports)]
pub(crate) use support::*;
#[allow(unused_imports)]
pub(crate) use tasks::*;
#[allow(unused_imports)]
pub(crate) use thermal::*;

#[cfg(test)]
#[path = "runtime_tests.rs"]
mod tests;
