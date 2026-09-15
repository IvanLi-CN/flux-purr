// Runtime assembly is kept in responsibility-oriented sections. The files are
// included in dependency order so target-gated firmware symbols retain the
// original private visibility and no runtime behavior changes.
include!("support.rs");
include!("eeprom_snapshot.rs");
include!("frontpanel.rs");
include!("thermal.rs");
include!("fan.rs");
include!("pd_control.rs");
include!("adc.rs");
include!("pd_protocol.rs");
include!("eeprom.rs");
include!("power.rs");
include!("tasks.rs");
include!("control_plane.rs");
include!("lan.rs");
include!("display_io.rs");
include!("boot.rs");
include!("runtime_loop.rs");

#[cfg(test)]
#[path = "runtime_tests.rs"]
mod tests;
