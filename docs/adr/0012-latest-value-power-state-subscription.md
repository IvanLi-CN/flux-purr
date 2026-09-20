# Latest-Value Power State Subscription

## Status

Accepted

## Context

Display, heater control, fan control, and the control-plane status projection must receive shared power information without polling another module. They need an initial complete state and then only the newest self-consistent state; none needs a historical trace of every PD observation.

## Decision

`PowerCoordinator` privately writes `embassy_sync::watch::Watch<CriticalSectionRawMutex, PowerState, 5>`. Four named subscribers are thermal control, fan policy, Front Panel display, and control-plane status; one receiver remains available for a future consumer. A subscriber receives the initial state with `get().await`, then awaits `changed()`. `PowerState` is a latest-value projection: intermediate updates may coalesce, while a slow subscriber always reconciles from the latest complete snapshot. The raw Embassy primitive is hidden by the power-domain subscription adapter.

## Consequences

The state transport uses one stored `PowerState`, not a message queue or one state copy per subscriber. Adding a sixth asynchronous subscriber requires an explicit static-capacity review. Contract completion and safety revocation cannot rely on this transport, because its intentionally coalescing semantics do not preserve a terminal-event history.
