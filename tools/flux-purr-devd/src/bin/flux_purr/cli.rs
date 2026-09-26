use std::{
    fs::{self, File},
    io::{self, BufRead, BufReader, BufWriter, IsTerminal, Read, Write},
    net::Ipv4Addr,
    path::{Path, PathBuf},
    process::{Child, Command as ProcessCommand, Stdio},
    sync::{Arc, Mutex},
    time::{Duration, Instant as StdInstant, SystemTime, UNIX_EPOCH},
};

use clap::{ArgAction, ArgGroup, Args, Parser, Subcommand, ValueEnum};
use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, MouseButton,
        MouseEventKind,
    },
    execute, queue,
    style::{Attribute, Print, SetAttribute},
    terminal::{self, Clear, ClearType},
};
use flux_purr_devd::{
    DEFAULT_DEVD_ENDPOINT, WifiConfigOp, developer_backup,
    firmware_bundle::{self, INTEGRITY_CATALOG_FILE},
    hardware_registry_path,
    lan::{
        LanDeviceConfig, LanPairRequest, LanScanRequest, authorized_json, device_from_discovery,
        discover_cidr, discover_mdns, merge_lan_device, pair_device,
    },
    local_control_request, local_control_request_bytes, read_user_config,
    validate_local_control_endpoint, write_user_config,
};
#[cfg(test)]
use flux_purr_devd::{FirmwareArtifact, FirmwareArtifactCatalog};
#[cfg(test)]
use reqwest::Url;
use reqwest::{Client, Method};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

#[path = "thermal_flagship.rs"]
mod thermal_flagship;
#[path = "thermal_report.rs"]
mod thermal_report;
#[path = "thermal_retune.rs"]
mod thermal_retune;

#[path = "cli/args.rs"]
pub(crate) mod args;
#[path = "cli/buzzer.rs"]
pub(crate) mod buzzer;
#[path = "cli/calibration.rs"]
pub(crate) mod calibration;
#[path = "cli/calibration_capture.rs"]
pub(crate) mod calibration_capture;
#[path = "cli/device_ops.rs"]
pub(crate) mod device_ops;
#[path = "cli/presentation.rs"]
pub(crate) mod presentation;
#[path = "cli/ram_run.rs"]
pub(crate) mod ram_run;
#[path = "cli/thermal_model.rs"]
pub(crate) mod thermal_model;
#[path = "cli/thermal_workflow.rs"]
pub(crate) mod thermal_workflow;
#[path = "cli/transport.rs"]
pub(crate) mod transport;

pub(crate) use args::*;
pub(crate) use buzzer::*;
pub(crate) use calibration::*;
pub(crate) use calibration_capture::*;
pub(crate) use device_ops::*;
pub(crate) use presentation::*;
pub(crate) use ram_run::*;
pub(crate) use thermal_model::*;
pub(crate) use thermal_workflow::*;
pub(crate) use transport::*;

#[cfg(test)]
#[path = "cli_tests.rs"]
mod tests;
