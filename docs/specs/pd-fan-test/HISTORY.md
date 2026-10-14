# PD Fan Test History

The topic was introduced as the standalone fan-PD session described by
`docs/adr/0017-pd-fan-fixed-selection.md`. It keeps the existing general fan
bring-up command and the complete PD sink HIL as separate contracts while
sharing only transport-neutral PD wire helpers.
