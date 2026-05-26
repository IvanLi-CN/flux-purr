# Product

## Register

product

## Users

Flux Purr is used by hardware and firmware operators working with an ESP32-S3 thermal bench device. They are usually validating live runtime state, provisioning WiFi, checking firmware artifacts, or confirming safety boundaries while a device is connected over USB and native devd.

## Product Purpose

The product provides a focused embedded-device control console for a small thermal bench. Success means the operator can see the real device state, adjust safe runtime settings, provision WiFi, and verify firmware artifacts without mistaking mock data, stale feedback, or disabled hardware for live control.

## Brand Personality

Precise, physical, restrained. The interface should feel like a compact industrial instrument: tactile enough to read as hardware-adjacent, disciplined enough to be trusted around power, heat, USB leases, and firmware operations.

## Anti-references

Do not make this a fleet management dashboard, marketing landing page, generic SaaS settings screen, or decorative glass UI. Do not hide standard form affordances behind stylized labels. Do not make live hardware state look like simulated mock telemetry.

## Design Principles

- Real state beats optimistic appearance.
- Standard controls stay recognizable, especially inputs, switches, segmented controls, and destructive actions.
- Safety boundaries are visible at the point of action.
- Dense information is acceptable when it helps bench work, but the screen must keep a light-tool mental model.
- Industrial styling supports the task; it must not reduce clarity.

## Accessibility & Inclusion

Interactive controls should expose semantic labels and visible focus states. Touch targets should remain at least 48px on mobile surfaces. Color should reinforce state but not be the only indicator; disabled, selected, warning, and success states need shape or text differences as well.
