# Buzzer Arbiter and Realtime Task

## Responsibility

Arbitrate one physical cue stream, advance cue steps, and preserve Timer2
carrier transitions without allowing callers to write raw PWM.

## Inputs and Reads

- Feedback requests.
- Protection and attention safety signals.
- Cue deadlines.
- Optional feature-gated buzzer-test commands.

## Commands and Mutations

Bounded feedback, protection, reminder, and safety-signal commands. The arbiter
may start, queue, coalesce, replace, preempt, suppress, or stop a cue.

## Published Outputs

- `BuzzerDecision` and `BuzzerOutput`.
- Optional test status/trace.
- Applied GPIO48 PWM state.

## Mechanism

`BUZZER_COMMANDS` capacity `32` plus the latest-value
`BUZZER_SAFETY_COMMAND` signal. The dedicated task wakes at cue deadlines.

## Control Authority

`BuzzerArbiter` owns cue selection. `run_buzzer_task` owns the arbiter, Timer2,
and every GPIO48 PWM write.

## Boundary Classification

| Boundary | Classification | Contract |
| --- | --- | --- |
| `BUZZER_COMMANDS` | Command | Requests a cue subject to arbitration. |
| `BuzzerDecision` | Snapshot/decision | Describes the selected cue step. |
| `BUZZER_SAFETY_COMMAND` | Interlock | Protection/attention can suppress or preempt feedback. |

## Physical Output Ownership

Only `run_buzzer_task` and `apply_buzzer_output` write the buzzer timer and
GPIO48 PWM. Runtime callers submit requests.

## Safety and Failure Behavior

Protection alarm has highest priority. Ordinary feedback is dropped outside the
normal audible safety state. Thermal attention can stop the alarm, retain an
acknowledgement state, and replay reminders without replaying stale feedback.

## Source References

- `firmware/src/buzzer.rs`: `BuzzerArbiter`, `ProtectionAlarmCadence`
- `firmware/src/bin/flux_purr/tasks.rs`: `BUZZER_COMMANDS`,
  `BUZZER_SAFETY_COMMAND`, `run_buzzer_task`, `apply_buzzer_output`
