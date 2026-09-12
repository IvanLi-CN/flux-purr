# ESP32-S3 RAM Bring-up Firmware Research

## Scope

This note establishes the execution, security, and runtime-identity facts for
an ESP32-S3 `flux-purr-ram-bringup` firmware. It covers the repository's
locked `espflash 4.5.0`, Espressif's documented ROM download protocol, and the
existing Flux Purr USB JSONL protocol. It does not assert the eFuse state of a
physical Device; that requires a read from the explicitly authorized Device.

## Conclusions

- `espflash flash --ram` is a RAM-load-and-execute operation. In the pinned
  version it sends only `MEM_BEGIN`, `MEM_DATA`, and `MEM_END` for the image
  segments, rather than using the erase/write Flash path. It does not erase or
  write SPI Flash.
- A RAM image must be a separate, all-IRAM/all-DRAM ELF. `espflash 4.5.0`
  rejects an ELF with any SPI-Flash-mapped segment; Espressif also documents
  that such an image will not load correctly.
- RAM execution is temporary. On a subsequent reset, the ESP32-S3 follows its
  boot strap. With normal straps it starts the program stored in Flash, so a
  completed RAM bring-up run does not replace the installed product firmware.
- Secure UART Download mode forbids the memory commands and arbitrary code
  execution needed for RAM loading. Permanently disabling ROM Download Mode
  prevents the operation altogether. A native USB route also depends on its
  USB download capability remaining enabled.
- Neither the documented ROM protocol nor `espflash` can identify whether the
  application that was running before ROM download was a product firmware or a
  RAM image. The CLI must identify a live application before it resets the
  chip, through the application's USB JSONL protocol.
- Add a required `firmwareKind` field to the existing `get_identity` response.
  Its initial values should be `product` and `ram_bringup`. It is a statement
  from the running application, not a ROM-attested measurement of residency.

## Pinned `espflash` Behavior

The workspace locks `espflash` to exactly `4.5.0` in
`tools/flux-purr-devd/Cargo.toml:34`; Cargo records the crates.io checksum in
`Cargo.lock:1729-1733`.

`FlashArgs.ram` is the `--ram` option ([source][espflash-flash-args]). The
command selects `load_elf_to_ram` instead of constructing and flashing an
image ([source][espflash-flash-command]). The relevant local source is:

- `/Users/ivan/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/espflash-4.5.0/src/bin/espflash.rs:298-333`
- `/Users/ivan/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/espflash-4.5.0/src/flasher/mod.rs:1091-1117`
- `/Users/ivan/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/espflash-4.5.0/src/target/flash_target/ram.rs:36-93`

`load_elf_to_ram` parses the input as an ELF and rejects it with
`ElfNotRamLoadable` when it finds a Flash-mapped section. Its segment selector
is deliberately simple: it includes only sections whose addresses are not
Flash addresses ([source][espflash-ram-segments]). The bring-up linker layout
therefore remains responsible for placing every executable and initialized
data section in valid internal IRAM or DRAM; a non-Flash address alone is not
an independently proven RAM-budget check.

For every accepted segment, `RamTarget` sends `MEM_BEGIN` and `MEM_DATA` with
the segment address. `MEM_END` receives the ELF entry point and executes it
([source][espflash-ram-target]). The implementation contains no Flash erase or
write command in this path. Its own API contract says that `load_elf_to_ram`
does not touch device Flash ([source][espflash-load-ram]). The enclosing CLI
may still obtain board information or detect Flash geometry before it branches
to RAM loading; that is not an erase or a Flash write.

This confirms the required CLI vocabulary:

- **RAM load**: transfer an all-RAM ELF with ROM memory commands and execute
  it. It does not change the installed product image.
- **Flash write**: erase or program SPI Flash. It changes the installed
  product image and must stay opt-in under the repository's existing safety
  boundary.

Espressif documents the same ROM protocol distinction: `MEM_BEGIN`,
`MEM_DATA`, and `MEM_END` load RAM and can execute it, whereas `FLASH_BEGIN`
and `FLASH_DATA` write SPI Flash ([serial protocol][esptool-writing-data]).
The `MEM_END` command carries an execute flag and entry address
([serial protocol][esptool-command-table]).

## Image and Reset Limits

Espressif's RAM-load documentation requires the input image to contain only
IRAM- and DRAM-resident segments. It warns that SPI-Flash-mapped segments will
not load correctly, and that the software loader can overlap the program's
RAM. `--no-stub` avoids that overlap on tools which use the software loader
([load-ram documentation][esptool-load-ram]). `espflash 4.5.0` is stricter
about Flash-mapped ELF sections: it rejects them before the transfer.

The RAM program is not a replacement boot image. ESP32-S3 enters the serial
bootloader only when GPIO0 is held low on reset; otherwise it runs the program
in Flash ([boot mode selection][esptool-boot-mode]). Therefore a reset after a
RAM bring-up run returns to the installed Flash firmware under the normal boot
straps. Power loss has the same practical consequence because the RAM image is
volatile.

## ROM Download Preconditions

The following are hardware properties rather than Cargo features. They must be
treated as a preflight result for the exact Device, never inferred from the
source tree.

| Condition | Effect on RAM bring-up | Evidence |
| --- | --- | --- |
| ROM Download Mode enabled and the selected download transport available | The ROM can receive the RAM memory commands after reset into download mode. | [ESP32-S3 boot mode selection][esptool-boot-mode] |
| `ENABLE_SECURITY_DOWNLOAD` set, or a security configuration that activates Secure UART Download mode | RAM memory writes and arbitrary RAM execution are rejected. `--no-stub` is required to use the restricted loader at all, but it does not make RAM loading permitted. | [ESP-IDF security overview][idf-uart-download], [esptool secure-download command set][esptool-secure-download] |
| `DIS_DOWNLOAD_MODE` set | ROM Download Mode is permanently disabled; booting with the download straps reports an error. | [ESP-IDF eFuse API][idf-disable-download] |
| `DIS_USB_SERIAL_JTAG_DOWNLOAD_MODE` set and the chosen path is USB Serial/JTAG | The ROM download path over USB is disabled. A UART path can be a separate question. | [ESP32-S3 eFuse field list][idf-efuse-summary] |
| `DIS_FORCE_DOWNLOAD` set | Hardware or software mechanisms that force download mode are disabled; this can remove an automatic-entry path even when the underlying mode is not otherwise asserted unavailable. | [ESP32-S3 eFuse field list][idf-efuse-summary] |

The ESP-IDF security overview says that Secure UART Download mode permits only
basic Flash-writing and security-information operations and does not allow
arbitrary downloaded code to execute. The esptool serial-protocol reference
explicitly lists RAM memory read/write and RAM loading among commands that
return an error in that mode. This makes a RAM bring-up image incompatible with
a security posture that enables Secure Download mode, regardless of whether
the image would otherwise fit.

The CLI may use ROM security information only for the prospective RAM-load
path. Entering ROM Download Mode resets the Device, so doing so first destroys
any currently executing RAM image. For a default no-write test command, query
the live application's identity first; perform ROM security preflight only
when a RAM load is actually necessary.

## Firmware Classification Is Not a ROM Feature

The ROM serial protocol starts by resetting the chip into the bootloader and
then identifies chip type, subtype, and revision ([serial protocol
initialization][esptool-initialization]). The documented command set describes
chip and security operations, not an application identity or the residency of
the application interrupted by reset. `get_security_info` reports security
features, not an application kind ([advanced commands][esptool-security-info]).

The pinned `espflash` source agrees: its `DeviceInfo` reports chip, revision,
crystal frequency, Flash size, features, and MAC address at
`/Users/ivan/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/espflash-4.5.0/src/flasher/mod.rs:1070-1088`.
It has no application-kind or execution-residency field. Its RAM loader starts
only after the ROM connection is already established.

Consequently, a ROM probe cannot answer either of these questions:

- Was the application before reset `flux-purr` or `flux-purr-ram-bringup`?
- Was that application executing from Flash or RAM?

The CLI must not infer a product firmware from a ROM response, an absent USB
JSONL response, a port name, a MAC address, or the currently installed Flash
image. None proves what had been running before the reset.

## Supported Runtime Identity Mechanism

Flux Purr already has a compact, versioned USB JSONL identity exchange:

- `firmware/src/control_plane.rs:27-40` defines `flux-purr.usb.v1`, JSONL, and
  the shared bounded-line limits.
- `firmware/src/control_plane.rs:114-161` defines the serialized `Identity`
  object and its current product values.
- `firmware/src/control_plane.rs:2410-2446` accepts the `get_identity`
  request operation.
- `firmware/src/bin/flux_purr.rs:12233-12240` answers `get_identity` before
  the main runtime is ready. The same identity response is used during
  recovery and normal runtime at `firmware/src/bin/flux_purr.rs:12464-12470`
  and `firmware/src/bin/flux_purr.rs:12655-12660`.

The RAM bring-up firmware should implement the same minimal request/response
shape, with no EEPROM, heater, Wi-Fi, or Flash mutation involved in identity:

```json
{"type":"request","requestId":"ram-preflight","op":"get_identity"}
```

```json
{
  "type":"response",
  "requestId":"ram-preflight",
  "ok":true,
  "result":{
    "identity":{
      "firmwareKind":"ram_bringup",
      "protocolVersion":"flux-purr.usb.v1"
    }
  }
}
```

The exact production response retains all existing `Identity` fields. The
example omits them only to show the new discriminator.

The host's classification must remain explicit:

| Identity probe result before any reset | CLI classification | Permitted inference |
| --- | --- | --- |
| Valid matching JSONL response with `firmwareKind: "product"` | Product firmware is live. | The application self-identifies as the product image. It does not prove Flash contents or security state. |
| Valid matching JSONL response with `firmwareKind: "ram_bringup"` | RAM bring-up firmware is live. | The application self-identifies as the bring-up image. The CLI can use its test/preview protocol without ROM reset when the requested profile is supported. |
| Valid response but no `firmwareKind` | Legacy compatible application. | The application supports some JSONL protocol, but its kind is unknown. Do not silently treat it as product or RAM bring-up. |
| JSONL response rejects or does not recognize `get_identity` | Legacy or incompatible application protocol. | Its kind is unknown. |
| No matching JSONL response | Application identity unavailable. | It may be in ROM Download Mode, unresponsive, not connected to this USB data path, or running a non-JSONL legacy image. Its kind is unknown. |

This mirrors the host tool's existing EEPROM-preflight distinction between no
JSONL response, non-JSON output, and unmatched JSONL responses at
`tools/flux-purr-devd/src/bin/flux-purr.rs:2511-2523`, while avoiding an
unsupported claim that the host can identify a ROM-resident image.

## `firmwareKind` Recommendation

Add `firmwareKind` as a required field in the `Identity` value, serialized as
camel-case JSON by the existing `Identity` serialization rule. Define these
closed initial values:

| Value | Meaning | CLI consequence |
| --- | --- | --- |
| `product` | The normal `flux-purr` application family. | The RAM-test CLI may perform its separate RAM-load flow when it needs a bring-up profile. It must not call RAM-only test commands against this identity. |
| `ram_bringup` | The dedicated all-RAM `flux-purr-ram-bringup` application family. | The CLI may request only explicitly advertised bring-up profiles, and can avoid resetting/reloading when the desired profile is already running. |

`firmwareKind` must not be overloaded with build version, profile name,
capability set, or an assertion independently verified by ROM. Keep
`firmwareVersion`, `buildId`, `gitSha`, and `capabilities` for those existing
purposes. If one bring-up binary contains multiple previews, expose the active
or supported preview through a separate bounded field or capability; do not
turn each preview into a new firmware kind.

The CLI should make the operational decision in this order:

1. Query `get_identity` without resetting the exact requested port.
2. Reuse a live `ram_bringup` instance only when its declared capabilities
   cover the requested preview or test.
3. For a product, legacy, unknown, or unavailable identity, state the observed
   classification. Only enter ROM Download Mode when the command is authorized
   to load the RAM image.
4. After a RAM load, wait for a matching `ram_bringup` identity response before
   issuing a test or preview command. A successful ROM transfer alone is not
   evidence that the expected firmware is running.
5. Keep any Flash-write option separate, named as a Flash write, and require
   its existing explicit authorization. A normal RAM load is not a Flash write.

## Sources

- [Espressif esptool: Load a Binary to RAM](https://docs.espressif.com/projects/esptool/en/latest/esp32s3/esptool/advanced-commands.html#load-a-binary-to-ram-load-ram)
- [Espressif esptool: Serial Protocol, command table](https://docs.espressif.com/projects/esptool/en/latest/esp32s3/advanced-topics/serial-protocol.html#command-opcode)
- [Espressif esptool: Serial Protocol, writing data](https://docs.espressif.com/projects/esptool/en/latest/esp32s3/advanced-topics/serial-protocol.html#writing-data)
- [Espressif esptool: Secure Download Mode command restrictions](https://docs.espressif.com/projects/esptool/en/latest/esp32s3/advanced-topics/serial-protocol.html#supported-in-secure-download-mode)
- [Espressif esptool: ESP32-S3 boot mode selection](https://docs.espressif.com/projects/esptool/en/latest/esp32s3/advanced-topics/boot-mode-selection.html#gpio0)
- [ESP-IDF: ESP32-S3 UART Download Mode security](https://docs.espressif.com/projects/esp-idf/en/stable/esp32s3/security/security.html#uart-download-mode)
- [ESP-IDF: disable ROM Download Mode eFuse API](https://docs.espressif.com/projects/esp-idf/en/stable/esp32s3/api-reference/system/efuse.html#_CPPv435esp_efuse_disable_rom_download_modev)
- [ESP-IDF: ESP32-S3 eFuse summary](https://docs.espressif.com/projects/esp-idf/en/stable/esp32s3/api-reference/system/efuse.html#efuse-api)
- [espflash v4.5.0: flash command](https://github.com/esp-rs/espflash/blob/v4.5.0/espflash/src/bin/espflash.rs#L298-L332)
- [espflash v4.5.0: RAM loader](https://github.com/esp-rs/espflash/blob/v4.5.0/espflash/src/flasher/mod.rs#L1091-L1117)
- [espflash v4.5.0: RAM target](https://github.com/esp-rs/espflash/blob/v4.5.0/espflash/src/target/flash_target/ram.rs#L36-L93)
- [espflash v4.5.0: ELF RAM-segment selection](https://github.com/esp-rs/espflash/blob/v4.5.0/espflash/src/image_format/mod.rs#L210-L216)

[espflash-flash-args]: https://github.com/esp-rs/espflash/blob/v4.5.0/espflash/src/cli/mod.rs#L149-L172
[espflash-flash-command]: https://github.com/esp-rs/espflash/blob/v4.5.0/espflash/src/bin/espflash.rs#L298-L333
[espflash-load-ram]: https://github.com/esp-rs/espflash/blob/v4.5.0/espflash/src/flasher/mod.rs#L1091-L1117
[espflash-ram-segments]: https://github.com/esp-rs/espflash/blob/v4.5.0/espflash/src/image_format/mod.rs#L210-L216
[espflash-ram-target]: https://github.com/esp-rs/espflash/blob/v4.5.0/espflash/src/target/flash_target/ram.rs#L36-L93
[esptool-writing-data]: https://docs.espressif.com/projects/esptool/en/latest/esp32s3/advanced-topics/serial-protocol.html#writing-data
[esptool-command-table]: https://docs.espressif.com/projects/esptool/en/latest/esp32s3/advanced-topics/serial-protocol.html#commands
[esptool-load-ram]: https://docs.espressif.com/projects/esptool/en/latest/esp32s3/esptool/advanced-commands.html#load-a-binary-to-ram-load-ram
[esptool-boot-mode]: https://docs.espressif.com/projects/esptool/en/latest/esp32s3/advanced-topics/boot-mode-selection.html#gpio0
[idf-uart-download]: https://docs.espressif.com/projects/esp-idf/en/stable/esp32s3/security/security.html#uart-download-mode
[esptool-secure-download]: https://docs.espressif.com/projects/esptool/en/latest/esp32s3/advanced-topics/serial-protocol.html#supported-in-secure-download-mode
[idf-disable-download]: https://docs.espressif.com/projects/esp-idf/en/stable/esp32s3/api-reference/system/efuse.html#_CPPv435esp_efuse_disable_rom_download_modev
[idf-efuse-summary]: https://docs.espressif.com/projects/esp-idf/en/stable/esp32s3/api-reference/system/efuse.html#efuse-api
[esptool-initialization]: https://docs.espressif.com/projects/esptool/en/latest/esp32s3/advanced-topics/serial-protocol.html#initialization
[esptool-security-info]: https://docs.espressif.com/projects/esptool/en/latest/esp32s3/esptool/advanced-commands.html#read-security-info-get-security-info
