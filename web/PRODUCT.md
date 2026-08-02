# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Users

Flux Purr Web App serves hardware and firmware operators working with one Flux Purr thermal bench device at a time. They use it while a device is connected over USB or reachable through the native devd bridge, and they need to distinguish live hardware state from mock, unavailable, or stale state.

## Product Purpose

Flux Purr Web App is the browser-based operating surface for the wider Flux Purr project. It lets an operator observe device state, control safe thermal behavior, perform calibration workflows, inspect firmware artifacts, and connect through Web Serial or devd without representing the firmware, hardware, and developer tooling as a whole.

Success means the operator can identify the active device and transport, understand whether feedback is live, complete a supported workflow, and see safety or capability boundaries before acting.

## Positioning

The Web App combines transport-aware device control, thermal safety feedback, calibration, and firmware inspection in a focused single-device workbench. It is not a generic fleet-management dashboard or a substitute for the rest of the Flux Purr system.

## Operating Context

- A bench operator works with a physically connected ESP32-S3 thermal device.
- Browser Web Serial and the native devd bridge are distinct transport paths with different capabilities.
- Mock scenarios support reproducible development and review, but must never appear to be live hardware.
- Device leases, serial availability, calibration state, firmware artifacts, and thermal protections can block actions.
- The interface is used at desktop workbench density and remains usable on narrow browser viewports.

## Capabilities and Constraints

- Observe device identity, transport, telemetry, thermal state, event history, and capability status.
- Adjust supported runtime thermal settings only when the active transport and device state allow it.
- Run VIN ADC, RTD ADC, and heater-curve calibration workflows with guarded unsaved state.
- Inspect and dry-check firmware artifacts; real flashing remains protected by the native developer workflow.
- Preserve explicit distinctions among live, mock, offline, degraded, pending, and unsupported states.
- Keep destructive or safety-sensitive actions visibly gated at the point of use.

## Brand Commitments

- Product surface name: Flux Purr Web App.
- Voice: precise, concise, physical, and restrained.
- The interface should read as a compact industrial instrument rather than a SaaS administration shell.
- Industrial detail must support hierarchy, state, or interaction; decoration alone is not a reason to add mechanical chrome.

## Evidence on Hand

- Runnable React implementation: `src/features/control-plane-demo/components/control-plane-demo.tsx`.
- Visual tokens and component states: `src/index.css`.
- Stable component scenarios: `src/stories/ControlPlaneDemo.stories.tsx`.
- Runtime and acceptance contract: `../docs/specs/hhwq8-web-control-plane-demo/` and `../docs/specs/m8r4q-real-control-plane-runtime/`.
- Existing mock, devd, and Web Serial scenarios are available in `src/features/control-plane-demo/`.
- No testimonials, customer claims, fleet-management claims, or marketing proof should be fabricated.

## Product Principles

- Real state beats optimistic appearance.
- Transport and capability boundaries stay visible.
- Safety information appears where decisions are made.
- Dense information is acceptable when it accelerates bench work.
- Standard controls remain recognizable and keyboard accessible.

## Accessibility & Inclusion

Interactive controls expose semantic labels and visible focus states. Mobile touch targets remain at least 48px. Color reinforces state but is never the only indicator; selected, disabled, warning, success, and failure states also use text, shape, or iconography.
