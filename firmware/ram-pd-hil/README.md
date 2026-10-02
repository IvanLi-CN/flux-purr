# Flux Purr PD Sink HIL RAM image

This image is a flash-free, capability-scoped ESP32-S3 RAM session for the
FUSB302B USB Power Delivery Sink path. It owns the PD controller only while a
`test_pd_sink` request is running, keeps the heater low, never accesses EEPROM,
and verifies an attached fixed 5V recovery contract without interrupting the
USB-C VBUS path before returning its terminal summary.

The image is intentionally separate from `ram-bringup`: existing bring-up
capabilities retain their `pd=untouched` contract.
