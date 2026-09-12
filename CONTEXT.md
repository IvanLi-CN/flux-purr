# Flux Purr

Flux Purr is an ESP32-S3 hardware controller with a persistent product runtime and
temporary, hardware-facing diagnostics.

## Language

**Product Firmware**:
The persistent `flux-purr` application installed in MCU Flash that provides the
device's normal control and safety behavior.
_Avoid_: main image, normal firmware

**RAM Bring-up Firmware**:
A separately built test application intended for RAM-loaded board diagnostics
and previews, with an explicit persistent-install path.
_Avoid_: RAM feature, main firmware test mode

**Device Preview**:
A visual state rendered by RAM Bring-up Firmware on the physical front panel
without replacing Product Firmware.
_Avoid_: host preview, preview firmware

**RAM Bring-up Session**:
The temporary interval from loading RAM Bring-up Firmware until the MCU next
resets into Product Firmware.
_Avoid_: installed test firmware, test mode

**Persistent Test Installation**:
RAM Bring-up Firmware written into the sole bootable application slot, replacing
Product Firmware until a Product Firmware image is explicitly flashed again.
_Avoid_: RAM session, reversible test install

**Firmware Kind**:
The runtime identity classification `product` or `ram_bringup` returned through
the USB JSONL identity protocol.
_Avoid_: detected from ROM, inferred firmware type
