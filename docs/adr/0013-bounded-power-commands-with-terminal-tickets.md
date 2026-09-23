# Bounded Power Commands With Terminal Tickets

## Status

Accepted

## Context

PowerState deliberately coalesces stale state updates, but a contract requester must learn exactly once whether its accepted command was confirmed, rejected, interrupted, or superseded. Commands must have fixed memory, non-blocking admission, and no silent loss.

## Decision

PowerCoordinator accepts public commands through a private `Channel<CriticalSectionRawMutex, PowerCommand, 6>` and sends at most one PD-service command through a private `Channel<CriticalSectionRawMutex, PdServiceCommand, 1>`. A non-blocking client wrapper returns `Busy` when admission is unavailable. An accepted contract command receives a reserved bounded ticket slot and completes exactly once through its private reply signal with `Confirmed`, `Rejected`, `TimedOut`, `TransportFault`, `Detached`, or `Superseded`; a result slot is not reused until the caller consumes the terminal outcome. Repeated capability refreshes join the one in-flight refresh through a fixed-capacity ticket list and share its terminal result. PD-service state uses a coalescing watch, while terminal reports use a bounded channel so state updates can never overwrite completions. The raw Channel, Watch, and Signal APIs are not application-facing contracts.

## Consequences

No accepted contract command can disappear into state coalescing, and a command flood cannot turn PD work into an unbounded queue. Every client must handle immediate admission failure and terminal asynchronous failure. The fixed capacities cover four contract owners, an explicit refresh, and an Idle transition; changing that model requires an explicit memory and backpressure review.
