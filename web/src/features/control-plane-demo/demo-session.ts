import type { DemoAction, DemoActionKind } from './demo-inspector-state'
import type { EventLogEntry } from './types'

export interface DemoSessionState {
  actions: DemoAction[]
  events: EventLogEntry[]
  generation: number
}

export const defaultDemoSessionState: DemoSessionState = {
  actions: [],
  events: [],
  generation: 0,
}

export type DemoSessionCommand =
  | { type: 'append-action'; kind: DemoActionKind; targetDeviceId: string; event: EventLogEntry }
  | { type: 'clear-results' }
  | { type: 'reset' }

export function reduceDemoSession(
  state: DemoSessionState,
  command: DemoSessionCommand
): DemoSessionState {
  if (command.type === 'clear-results') {
    return { actions: [], events: [], generation: state.generation + 1 }
  }
  if (command.type === 'reset') {
    return { actions: [], events: [], generation: state.generation + 1 }
  }
  return {
    ...state,
    actions: [
      ...state.actions,
      {
        kind: command.kind,
        targetDeviceId: command.targetDeviceId,
        sequence: state.actions.length + 1,
      },
    ],
    events: [...state.events, command.event].slice(-12),
  }
}
