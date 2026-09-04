# S3 fan cycle bring-up implementation

## Current coverage

- The host-testable four-phase fan cycle, GPIO35/36/34 assignment, normalized fan fields, and Xtensa build contract are documented.
- The bring-up binary and its host fallback preserve the original phase semantics without changing product runtime behavior.

## Validation

- Host firmware tests and the documented Xtensa build gate cover the fan-cycle state machine and board constants.

## Remaining gaps

- This bring-up topic is archived; later fan runtime behavior belongs to the maintained heater/runtime topic.
