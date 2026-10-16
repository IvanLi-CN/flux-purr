# One production controller for every qualified PPS source

## Status

Accepted. Supersedes [ADR-0015](0015-source-class-thermal-controller-selection.md).

This decision also supersedes the source-class controller selection and 5A-only model-assistance boundary in [ADR-0016](0016-calibrated-natural-cooling-in-profile-control.md); its passive-loss, Hold-recovery and bounded-search decisions continue to apply to the shared controller.

## Context

The Operator requires the same heating and parameter-tuning function on 3A and 5A Devices. A separate 3A plant PI ignores the tuned point-local parameters and cannot establish production equivalence. Both source classes therefore use the existing calibrated Warmup/Approach/Hold profile controller, with the same braking-first Approach, physical phase gates, bounded search and independent confirmation. The source class selects current limits and a separate saved parameter bank, never an alternative control law. This supersedes ADR-0015's production-controller split.

## Decision

Actual source capability, temperature-dependent resistance, thermal capacity, passive cooling and transport delay remain per-Device inputs. Source-class changes while armed disarm before resetting and selecting another bank. PD service, request ownership, voltage/PWM mapping, sensor protection, persistence and protected thermal-plant calibration remain authoritative and unchanged. A 3A request cannot claim 5A current or use another Device's model or candidate.

The RTD divider excitation is a board electrical parameter, distinct from ADC calibration. The two authorized bench configurations use 3328mV and 3208mV respectively; identifying a rail voltage must not fabricate manual ADC calibration points. The default image uses 3328mV; `rtd-supply-3208mv` selects 3208mV and status reports `rtdDividerSupplyMv`. The board image changes this electrical input, never the control law.

## Consequences

Each group retains its own firmware artifact, calibration provenance, run identity, report and save receipt. Passing one group cannot qualify the other. The existing `65w` selector names the 3A parameter bank; a 60W source remains bounded by its actual negotiated capability, without promising 65W physical delivery.

## Alternatives considered

Keeping a separate 3A PI would ignore tuned point-local settings and violate the requirement for one heating function. Sharing calibration or candidates across boards would hide hardware differences and invalidate cold-initialization evidence.
