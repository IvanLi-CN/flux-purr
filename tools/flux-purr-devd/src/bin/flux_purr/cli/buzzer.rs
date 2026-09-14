async fn runtime_body(
    client: &Client,
    resolved: &ResolvedUsbTarget,
    args: RuntimeSetArgs,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let mut body = serde_json::Map::new();
    insert_if_some(&mut body, "targetTempC", args.target_temp_c);
    insert_if_some(&mut body, "selectedPresetSlot", args.selected_preset_slot);
    insert_if_some(&mut body, "activeCoolingEnabled", args.active_cooling);
    insert_if_some(&mut body, "postHeatCoolingMode", args.post_heat_cooling);
    insert_if_some(&mut body, "heatingFanGuardMode", args.heating_fan_guard);
    insert_if_some(&mut body, "heaterEnabled", args.heater_enabled);
    if args.fault_attention_acknowledged {
        body.insert("faultAttentionAcknowledged".to_string(), json!(true));
    }
    if let Some(file) = args.presets_file {
        body.insert("presetsC".to_string(), read_json_file(&file)?);
    }
    if args.preset_slot.is_some() || args.preset_temp_c.is_some() || args.preset_disabled {
        let slot = args
            .preset_slot
            .ok_or("preset edit requires --preset-slot")?;
        let status =
            request_with_lease(client, resolved.clone(), Method::GET, "/status", None).await?;
        let mut presets = status
            .get("presetsC")
            .and_then(Value::as_array)
            .cloned()
            .ok_or("status did not include presetsC")?;
        if slot >= presets.len() {
            return Err("preset slot is out of range".into());
        }
        presets[slot] = if args.preset_disabled {
            Value::Null
        } else {
            json!(
                args.preset_temp_c
                    .ok_or("preset edit requires --preset-temp-c or --preset-disabled")?
            )
        };
        body.insert("presetsC".to_string(), Value::Array(presets));
    }
    if body.is_empty() {
        return Err("runtime set requires at least one field".into());
    }
    Ok(Value::Object(body))
}

async fn buzzer_test(
    client: &Client,
    resolved: ResolvedUsbTarget,
    cue: Option<BuzzerCueArg>,
    scenario: Option<BuzzerScenarioArg>,
    repeat: bool,
    stop: bool,
    status: bool,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    buzzer_test_request(BuzzerRequestInput {
        client,
        resolved,
        cue,
        scenario,
        repeat,
        stop,
        status,
        capture_readback: true,
    })
    .await
}

async fn buzzer_test_live(
    client: &Client,
    resolved: ResolvedUsbTarget,
    cue: Option<BuzzerCueArg>,
    scenario: Option<BuzzerScenarioArg>,
    repeat: bool,
    stop: bool,
    status: bool,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    buzzer_test_request(BuzzerRequestInput {
        client,
        resolved,
        cue,
        scenario,
        repeat,
        stop,
        status,
        capture_readback: false,
    })
    .await
}

const BUZZER_CAPTURE_SETTLE_MS: u64 = 100;
const BUZZER_STOP_SETTLE_MS: u64 = 25;

fn buzzer_capture_delay(
    cue: Option<BuzzerCueArg>,
    scenario: Option<BuzzerScenarioArg>,
    repeat: bool,
    stop: bool,
    status: bool,
) -> Option<Duration> {
    if status {
        return None;
    }
    if stop {
        return Some(Duration::from_millis(BUZZER_STOP_SETTLE_MS));
    }
    if repeat {
        return cue.map(|cue| {
            Duration::from_millis(cue.one_shot_duration_ms() + BUZZER_CAPTURE_SETTLE_MS)
        });
    }
    if let Some(scenario) = scenario {
        return Some(Duration::from_millis(
            scenario.duration_ms() + BUZZER_CAPTURE_SETTLE_MS,
        ));
    }
    cue.map(|cue| Duration::from_millis(cue.one_shot_duration_ms() + BUZZER_CAPTURE_SETTLE_MS))
}

struct BuzzerRequestInput<'a> {
    client: &'a Client,
    resolved: ResolvedUsbTarget,
    cue: Option<BuzzerCueArg>,
    scenario: Option<BuzzerScenarioArg>,
    repeat: bool,
    stop: bool,
    status: bool,
    capture_readback: bool,
}

async fn buzzer_test_request(
    input: BuzzerRequestInput<'_>,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let BuzzerRequestInput {
        client,
        resolved,
        cue,
        scenario,
        repeat,
        stop,
        status,
        capture_readback,
    } = input;
    let (op, cue, scenario, repeat) = match (cue, scenario, repeat, stop, status) {
        (Some(cue), None, repeat, false, false) => {
            ("trigger", Some(cue.wire_value()), None, repeat)
        }
        (None, Some(scenario), false, false, false) => {
            ("run", None, Some(scenario.wire_value()), false)
        }
        (None, None, false, true, false) => ("stop", None, None, false),
        (None, None, false, false, true) => ("status", None, None, false),
        _ => {
            return Err(
                "buzzer test requires --cue [--repeat], --scenario, --stop, or --status".into(),
            );
        }
    };
    let lease = create_lease(client, &resolved).await?;
    let heartbeat = spawn_heartbeat(client.clone(), resolved.devd.clone(), lease.clone());
    let result = async {
        let mut result = request_leased(
            client,
            &resolved,
            &lease.lease_id,
            Method::POST,
            "/buzzer-test",
            Some(buzzer_test_body(op, cue, scenario, repeat)),
        )
        .await?;
        if capture_readback
            && let Some(delay) = buzzer_capture_delay(
                cue.and_then(buzzer_cue_arg_from_wire),
                scenario.and_then(buzzer_scenario_arg_from_wire),
                repeat,
                stop,
                status,
            )
        {
            // A diagnostic status exchange runs through the same USB executor
            // as firmware control. It must never land within an audible step.
            tokio::time::sleep(delay).await;
            result = request_leased(
                client,
                &resolved,
                &lease.lease_id,
                Method::POST,
                "/buzzer-test",
                Some(buzzer_test_body("status", None, None, false)),
            )
            .await?;
        }
        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(result)
    }
    .await;
    let _ = release_lease(client, &resolved.devd, &lease.lease_id).await;
    heartbeat.abort();
    let value = result?;
    if let Some(id) = resolved.hardware_id.as_deref() {
        let _ = remember_usb(id, &resolved.device, &resolved.devd);
    }
    Ok(value)
}

fn buzzer_cue_arg_from_wire(value: &str) -> Option<BuzzerCueArg> {
    BUZZER_CUE_CATALOG
        .iter()
        .find(|descriptor| descriptor.cue.wire_value() == value)
        .map(|descriptor| descriptor.cue)
}

fn buzzer_scenario_arg_from_wire(value: &str) -> Option<BuzzerScenarioArg> {
    BUZZER_SCENARIO_CATALOG
        .iter()
        .find(|descriptor| descriptor.scenario.wire_value() == value)
        .map(|descriptor| descriptor.scenario)
}

async fn buzzer_play_interactive(
    client: &Client,
    resolved: ResolvedUsbTarget,
    pointer_capture: bool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if io::stdin().is_terminal() && io::stdout().is_terminal() {
        return buzzer_play_terminal_interactive(client, resolved, pointer_capture).await;
    }
    buzzer_play_line_interactive(client, resolved).await
}

struct BuzzerTerminalGuard {
    pointer_capture: bool,
}

impl BuzzerTerminalGuard {
    fn enter(output: &mut impl Write, pointer_capture: bool) -> io::Result<Self> {
        terminal::enable_raw_mode()?;
        if let Err(error) = execute!(output, Hide) {
            let _ = terminal::disable_raw_mode();
            return Err(error);
        }
        let mut guard = Self {
            pointer_capture: false,
        };
        if pointer_capture && let Err(error) = guard.set_pointer_capture(output, true) {
            let _ = execute!(output, Show);
            let _ = terminal::disable_raw_mode();
            return Err(error);
        }
        Ok(guard)
    }

    fn set_pointer_capture(&mut self, output: &mut impl Write, enabled: bool) -> io::Result<()> {
        if self.pointer_capture == enabled {
            return Ok(());
        }
        if enabled {
            execute!(output, EnableMouseCapture)?;
        } else {
            execute!(output, DisableMouseCapture)?;
        }
        self.pointer_capture = enabled;
        output.flush()
    }
}

impl Drop for BuzzerTerminalGuard {
    fn drop(&mut self) {
        let mut output = io::stdout();
        if self.pointer_capture {
            let _ = execute!(output, DisableMouseCapture);
        }
        let _ = execute!(output, Show);
        let _ = terminal::disable_raw_mode();
    }
}

#[expect(
    clippy::excessive_nesting,
    reason = "CLI workflow or fixture preserves an ordered protocol scenario"
)]
async fn buzzer_play_terminal_interactive(
    client: &Client,
    resolved: ResolvedUsbTarget,
    pointer_capture: bool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut output = io::stdout();
    let mut terminal_guard = BuzzerTerminalGuard::enter(&mut output, pointer_capture)?;
    let mut selection = BuzzerTerminalSelection::default();
    let mut notice: Option<String> = None;
    let mut status =
        buzzer_test_live(client, resolved.clone(), None, None, false, false, true).await?;

    loop {
        render_buzzer_terminal(
            &mut output,
            &status,
            selection,
            notice.as_deref(),
            terminal_guard.pointer_capture,
        )?;
        let session_running = buzzer_session_state(&status) == "running";
        let mut action = None;

        loop {
            match event::read()? {
                Event::Key(key) => {
                    if matches!(key.kind, KeyEventKind::Press)
                        && matches!(key.code, KeyCode::Char('m') | KeyCode::Char('M'))
                    {
                        let enabled = !terminal_guard.pointer_capture;
                        terminal_guard.set_pointer_capture(&mut output, enabled)?;
                        notice = Some(if enabled {
                            "Pointer capture enabled. Press M again to release it for terminal copy."
                        } else {
                            "Pointer capture disabled. Terminal text selection and copy are enabled."
                        }
                        .to_string());
                        break;
                    }
                    if buzzer_terminal_move_selection(&mut selection, key.code, key.kind) {
                        break;
                    }
                    if let Some(next_action) =
                        buzzer_terminal_key_action(key.code, key.kind, selection, session_running)
                    {
                        action = Some(next_action);
                        break;
                    }
                }
                Event::Mouse(mouse) => match mouse.kind {
                    MouseEventKind::Down(MouseButton::Left) => {
                        if selection.select_row(mouse.row) {
                            break;
                        }
                        if let Some(next_action) = buzzer_terminal_pointer_action(
                            mouse.row,
                            mouse.column,
                            selection,
                            session_running,
                        ) {
                            action = Some(next_action);
                            break;
                        }
                    }
                    MouseEventKind::ScrollUp => {
                        selection.move_previous();
                        break;
                    }
                    MouseEventKind::ScrollDown => {
                        selection.move_next();
                        break;
                    }
                    _ => {}
                },
                Event::Resize(_, _) => break,
                _ => {}
            }
        }

        if let Some(action) = action {
            match execute_buzzer_interactive_action(client, resolved.clone(), action).await? {
                BuzzerInteractiveExecution::Exit => return Ok(()),
                BuzzerInteractiveExecution::Updated {
                    message,
                    status: next_status,
                } => {
                    notice = Some(message);
                    if let Some(next_status) = next_status {
                        status = next_status;
                    }
                }
            }
        }
    }
}

fn render_buzzer_terminal(
    output: &mut impl Write,
    status: &Value,
    selection: BuzzerTerminalSelection,
    notice: Option<&str>,
    pointer_capture: bool,
) -> io::Result<()> {
    let (columns, _) = terminal::size().unwrap_or((100, 30));
    let state = buzzer_session_state(status);
    let active_cue = status
        .get("activeCue")
        .and_then(Value::as_str)
        .unwrap_or("none");
    let selected_cue = status.get("cue").and_then(Value::as_str).unwrap_or("none");
    let selection_detail = match selection.item() {
        BuzzerTerminalItem::Cue(cue) => {
            let descriptor = buzzer_cue_descriptor(cue);
            format!("Selected cue: {} - {}", descriptor.label, descriptor.rhythm)
        }
        BuzzerTerminalItem::Scenario(scenario) => {
            let descriptor = buzzer_scenario_descriptor(scenario);
            format!(
                "Selected scenario: {} - {}",
                descriptor.label, descriptor.description
            )
        }
    };
    let default_input_help = if state == "running" {
        "Enter/Space stops continuous playback. M toggles pointer capture."
    } else {
        "Enter/Space plays once; C/L starts continuous playback. M toggles pointer capture."
    };
    let status_line = notice.unwrap_or(default_input_help);

    queue!(output, MoveTo(0, 0), Clear(ClearType::All))?;
    write_buzzer_terminal_line(output, 0, columns, "Flux Purr buzzer test")?;
    write_buzzer_terminal_line(
        output,
        1,
        columns,
        &format!("Session: {state}    Test cue: {selected_cue}    Active cue: {active_cue}"),
    )?;
    write_buzzer_terminal_line(output, 2, columns, &selection_detail)?;
    write_buzzer_terminal_line(output, 3, columns, status_line)?;
    write_buzzer_terminal_line(output, 4, columns, &buzzer_output_trace_summary(status))?;
    write_buzzer_terminal_line(output, 5, columns, "Production cues:")?;

    for (index, descriptor) in BUZZER_CUE_CATALOG.iter().enumerate() {
        write_buzzer_terminal_item_line(
            output,
            BUZZER_TERMINAL_CUE_START_ROW + index as u16,
            columns,
            selection.index == index,
            &format!("{} [{}]", descriptor.label, descriptor.kind),
        )?;
    }

    let scenario_header_row = buzzer_terminal_scenario_start_row() - 1;
    write_buzzer_terminal_line(
        output,
        scenario_header_row,
        columns,
        "Arbitration scenarios:",
    )?;
    for (index, descriptor) in BUZZER_SCENARIO_CATALOG.iter().enumerate() {
        write_buzzer_terminal_item_line(
            output,
            buzzer_terminal_scenario_start_row() + index as u16,
            columns,
            selection.index == BUZZER_CUE_CATALOG.len() + index,
            descriptor.label,
        )?;
    }

    write_buzzer_terminal_line(
        output,
        buzzer_terminal_actions_row(),
        columns,
        &format!(
            "{:<24}{:<18}{:<10}{:<14}{:<18}[Q] Exit",
            "[Enter/Space] Play/stop",
            "[C/L] Continuous",
            "[S] Stop",
            "[R] Refresh",
            buzzer_pointer_mode_label(pointer_capture),
        ),
    )?;
    write_buzzer_terminal_line(
        output,
        buzzer_terminal_actions_row() + 1,
        columns,
        "Mouse: click an item to select; click an action label to execute.",
    )?;
    output.flush()
}

const fn buzzer_pointer_mode_label(pointer_capture: bool) -> &'static str {
    if pointer_capture {
        "[M] Copy mode"
    } else {
        "[M] Pointer mode"
    }
}

fn write_buzzer_terminal_item_line(
    output: &mut impl Write,
    row: u16,
    columns: u16,
    selected: bool,
    label: &str,
) -> io::Result<()> {
    queue!(output, MoveTo(0, row))?;
    if selected {
        queue!(output, SetAttribute(Attribute::Reverse))?;
    }
    queue!(output, Print(truncate_buzzer_terminal_line(label, columns)))?;
    if selected {
        queue!(output, SetAttribute(Attribute::Reset))?;
    }
    Ok(())
}

fn write_buzzer_terminal_line(
    output: &mut impl Write,
    row: u16,
    columns: u16,
    line: &str,
) -> io::Result<()> {
    queue!(
        output,
        MoveTo(0, row),
        Print(truncate_buzzer_terminal_line(line, columns))
    )?;
    Ok(())
}

fn truncate_buzzer_terminal_line(line: &str, columns: u16) -> String {
    let limit = usize::from(columns.saturating_sub(1));
    if line.chars().count() <= limit {
        return line.to_string();
    }
    if limit <= 3 {
        return line.chars().take(limit).collect();
    }
    let mut truncated: String = line.chars().take(limit - 3).collect();
    truncated.push_str("...");
    truncated
}

async fn buzzer_play_line_interactive(
    client: &Client,
    resolved: ResolvedUsbTarget,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut status =
        buzzer_test_live(client, resolved.clone(), None, None, false, false, true).await?;
    loop {
        let action = {
            let stdin = io::stdin();
            let stdout = io::stdout();
            let mut input = stdin.lock();
            let mut output = stdout.lock();
            prompt_buzzer_play_action(&status, &mut input, &mut output)?
        };

        match execute_buzzer_interactive_action(client, resolved.clone(), action).await? {
            BuzzerInteractiveExecution::Exit => {
                println!("Buzzer test session closed without changing playback.");
                return Ok(());
            }
            BuzzerInteractiveExecution::Updated {
                message,
                status: next_status,
            } => {
                println!("{message}");
                if let Some(next_status) = next_status {
                    status = next_status;
                }
            }
        }
    }
}

enum BuzzerInteractiveExecution {
    Exit,
    Updated {
        message: String,
        status: Option<Value>,
    },
}

async fn execute_buzzer_interactive_action(
    client: &Client,
    resolved: ResolvedUsbTarget,
    action: BuzzerInteractiveAction,
) -> Result<BuzzerInteractiveExecution, Box<dyn std::error::Error + Send + Sync>> {
    match action {
        BuzzerInteractiveAction::Exit => Ok(BuzzerInteractiveExecution::Exit),
        BuzzerInteractiveAction::Refresh => {
            let status = buzzer_test_live(client, resolved, None, None, false, false, true).await?;
            Ok(BuzzerInteractiveExecution::Updated {
                message: "Session status refreshed.".to_string(),
                status: Some(status),
            })
        }
        BuzzerInteractiveAction::Stop => {
            let _ =
                buzzer_test_live(client, resolved.clone(), None, None, false, true, false).await?;
            let status = buzzer_test_live(client, resolved, None, None, false, false, true).await?;
            Ok(BuzzerInteractiveExecution::Updated {
                message: "Stop request applied.".to_string(),
                status: Some(status),
            })
        }
        BuzzerInteractiveAction::Play {
            cue,
            repeat,
            stop_current,
        } => {
            if stop_current {
                let _ = buzzer_test_live(client, resolved.clone(), None, None, false, true, false)
                    .await?;
            }
            let _ =
                buzzer_test_live(client, resolved, Some(cue), None, repeat, false, false).await?;
            let descriptor = buzzer_cue_descriptor(cue);
            let status = repeat.then(|| buzzer_interactive_repeat_status(cue));
            Ok(BuzzerInteractiveExecution::Updated {
                message: if repeat {
                    format!(
                        "Continuous {} playback started. Stop it with Enter, Space, or S; use R only when you need a readback.",
                        descriptor.label
                    )
                } else {
                    format!(
                        "Triggered {} through the production arbiter. Press again to reproduce rapid hardware input; use R after playback for its readback.",
                        descriptor.label
                    )
                },
                status,
            })
        }
        BuzzerInteractiveAction::RunScenario {
            scenario,
            stop_current,
        } => {
            if stop_current {
                let _ = buzzer_test_live(client, resolved.clone(), None, None, false, true, false)
                    .await?;
            }
            let status =
                buzzer_test(client, resolved, None, Some(scenario), false, false, false).await?;
            Ok(BuzzerInteractiveExecution::Updated {
                message: format!(
                    "Completed {}. Firmware session state: {}.",
                    buzzer_scenario_descriptor(scenario).label,
                    buzzer_session_state(&status),
                ),
                status: Some(status),
            })
        }
    }
}

fn buzzer_interactive_repeat_status(cue: BuzzerCueArg) -> Value {
    json!({
        "state": "running",
        "cue": cue.wire_value(),
        "repeat": true,
        "activeCue": cue.wire_value(),
        "trace": [],
        "outputTrace": [],
    })
}

fn prompt_buzzer_play_action<R: BufRead, W: Write>(
    status: &Value,
    input: &mut R,
    output: &mut W,
) -> Result<BuzzerInteractiveAction, Box<dyn std::error::Error + Send + Sync>> {
    write_buzzer_session_status(status, output)?;
    let is_running = status.get("state").and_then(Value::as_str) == Some("running");

    if is_running {
        writeln!(
            output,
            "The running session will not be stopped automatically."
        )?;
        writeln!(output, "  1) Refresh session status")?;
        writeln!(output, "  2) Stop active playback")?;
        writeln!(output, "  3) Replace playback after an explicit stop")?;
        writeln!(output, "  4) Exit without changing playback")?;
        match prompt_menu_choice(input, output, "Choose action", 4)? {
            1 => Ok(BuzzerInteractiveAction::Refresh),
            2 => Ok(BuzzerInteractiveAction::Stop),
            3 => prompt_buzzer_session_start(input, output, true),
            4 => Ok(BuzzerInteractiveAction::Exit),
            _ => unreachable!("menu choice is range-checked"),
        }
    } else {
        writeln!(output, "  1) Play a production buzzer cue")?;
        writeln!(output, "  2) Run feedback-arbitration scenario")?;
        writeln!(output, "  3) Refresh session status")?;
        writeln!(output, "  4) Exit without changing playback")?;
        match prompt_menu_choice(input, output, "Choose action", 4)? {
            1 => prompt_buzzer_cue(input, output, false),
            2 => prompt_buzzer_scenario(input, output, false),
            3 => Ok(BuzzerInteractiveAction::Refresh),
            4 => Ok(BuzzerInteractiveAction::Exit),
            _ => unreachable!("menu choice is range-checked"),
        }
    }
}

fn write_buzzer_session_status<W: Write>(status: &Value, output: &mut W) -> io::Result<()> {
    let active_cue = status
        .get("activeCue")
        .and_then(Value::as_str)
        .unwrap_or("none");
    let selected_cue = status.get("cue").and_then(Value::as_str).unwrap_or("none");
    let state = buzzer_session_state(status);

    writeln!(output, "Buzzer test session")?;
    writeln!(output, "  State: {state}")?;
    writeln!(output, "  Selected cue: {selected_cue}")?;
    writeln!(output, "  Active cue: {active_cue}")?;
    if status.get("repeat").and_then(Value::as_bool) == Some(true) {
        writeln!(output, "  Mode: continuous")?;
    }
    if state == "running" && active_cue == "none" {
        writeln!(
            output,
            "  PWM output is silent between production cue steps or cadence bursts."
        )?;
    }
    if selected_cue == "attention_reminder" && active_cue == "none" && state == "running" {
        writeln!(
            output,
            "  Waiting for the production 10-second attention cadence."
        )?;
    }
    if let Some(trace) = status.get("trace").and_then(Value::as_array)
        && !trace.is_empty()
    {
        writeln!(output, "  Arbitration trace:")?;
        for event in trace {
            let elapsed = event.get("elapsedMs").and_then(Value::as_u64).unwrap_or(0);
            let decision = event.get("decision").and_then(Value::as_object);
            let source = decision
                .and_then(|value| value.get("source"))
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            let cue = decision
                .and_then(|value| value.get("cue"))
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            let disposition = decision
                .and_then(|value| value.get("disposition"))
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            writeln!(
                output,
                "    {elapsed:>4} ms  {source} / {cue} / {disposition}"
            )?;
        }
    }
    if let Some(trace) = status.get("outputTrace").and_then(Value::as_array)
        && !trace.is_empty()
    {
        writeln!(output, "  MCPWM timer2 output trace:")?;
        for event in trace {
            let elapsed = event.get("elapsedMs").and_then(Value::as_u64).unwrap_or(0);
            let requested = event
                .get("requestedFrequencyHz")
                .and_then(Value::as_u64)
                .map(|value| format!("{value} Hz"))
                .unwrap_or_else(|| "silent".to_string());
            let applied = event
                .get("appliedFrequencyHz")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let observed = event
                .get("observedFrequencyHz")
                .and_then(Value::as_u64)
                .map(|value| format!("{value} Hz"))
                .unwrap_or_else(|| "pending".to_string());
            let observed_edges = event
                .get("observedRisingEdges")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let observed_window_ms = event
                .get("observedWindowMs")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let duty = event
                .get("dutyPercent")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let prescaler = event
                .get("timerPrescaler")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let period = event
                .get("timerPeriodTicks")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            writeln!(
                output,
                "    {elapsed:>4} ms  requested={requested:<10} timer={applied:>4} Hz  pad={observed:<10} ({observed_edges} edges/{observed_window_ms} ms)  duty={duty:>3}%  cfg={prescaler}/{period}"
            )?;
        }
    }
    Ok(())
}

fn buzzer_output_trace_summary(status: &Value) -> String {
    let Some(trace) = status.get("outputTrace").and_then(Value::as_array) else {
        return "MCPWM timer2 readback: unavailable on this firmware.".to_string();
    };
    let Some(last) = trace.last() else {
        return "MCPWM timer2 readback: unavailable on this firmware.".to_string();
    };
    let requested = last
        .get("requestedFrequencyHz")
        .and_then(Value::as_u64)
        .map(|value| format!("{value} Hz"))
        .unwrap_or_else(|| "silent".to_string());
    let applied = last
        .get("appliedFrequencyHz")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let observed = trace
        .iter()
        .rev()
        .find_map(|event| event.get("observedFrequencyHz").and_then(Value::as_u64))
        .map(|value| format!("{value} Hz"))
        .unwrap_or_else(|| "pending".to_string());
    let duty = last.get("dutyPercent").and_then(Value::as_u64).unwrap_or(0);
    format!("GPIO48: requested {requested}, timer {applied} Hz, pad {observed}, duty {duty}%")
}

fn buzzer_session_state(status: &Value) -> &str {
    status
        .get("state")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
}

fn prompt_buzzer_session_start<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    stop_current: bool,
) -> Result<BuzzerInteractiveAction, Box<dyn std::error::Error + Send + Sync>> {
    writeln!(output, "  1) Play a production buzzer cue")?;
    writeln!(output, "  2) Run feedback-arbitration scenario")?;
    match prompt_menu_choice(input, output, "Start", 2)? {
        1 => prompt_buzzer_cue(input, output, stop_current),
        2 => prompt_buzzer_scenario(input, output, stop_current),
        _ => unreachable!("menu choice is range-checked"),
    }
}

fn prompt_buzzer_cue<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    stop_current: bool,
) -> Result<BuzzerInteractiveAction, Box<dyn std::error::Error + Send + Sync>> {
    writeln!(output, "Production buzzer cue catalogue:")?;
    for (index, descriptor) in BUZZER_CUE_CATALOG.iter().enumerate() {
        writeln!(
            output,
            "  {}) {} [{}] - {}",
            index + 1,
            descriptor.label,
            descriptor.kind,
            descriptor.rhythm,
        )?;
    }
    let cue_index = prompt_menu_choice(input, output, "Cue", BUZZER_CUE_CATALOG.len())?;
    let descriptor = BUZZER_CUE_CATALOG[cue_index - 1];

    writeln!(
        output,
        "Selected: {} ({})",
        descriptor.label, descriptor.rhythm
    )?;
    writeln!(output, "  1) Play once")?;
    writeln!(output, "  2) Play continuously (explicit stop required)")?;
    let repeat = match prompt_menu_choice(input, output, "Playback mode", 2)? {
        1 => false,
        2 => true,
        _ => unreachable!("menu choice is range-checked"),
    };
    Ok(BuzzerInteractiveAction::Play {
        cue: descriptor.cue,
        repeat,
        stop_current,
    })
}

fn prompt_buzzer_scenario<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    stop_current: bool,
) -> Result<BuzzerInteractiveAction, Box<dyn std::error::Error + Send + Sync>> {
    writeln!(output, "Feedback-arbitration scenarios:")?;
    for (index, descriptor) in BUZZER_SCENARIO_CATALOG.iter().enumerate() {
        writeln!(
            output,
            "  {}) {} - {}",
            index + 1,
            descriptor.label,
            descriptor.description,
        )?;
    }
    let scenario_index =
        prompt_menu_choice(input, output, "Scenario", BUZZER_SCENARIO_CATALOG.len())?;
    Ok(BuzzerInteractiveAction::RunScenario {
        scenario: BUZZER_SCENARIO_CATALOG[scenario_index - 1].scenario,
        stop_current,
    })
}

fn buzzer_cue_descriptor(cue: BuzzerCueArg) -> &'static BuzzerCueDescriptor {
    BUZZER_CUE_CATALOG
        .iter()
        .find(|descriptor| descriptor.cue == cue)
        .expect("every CLI buzzer cue has a catalogue descriptor")
}

fn buzzer_scenario_descriptor(scenario: BuzzerScenarioArg) -> &'static BuzzerScenarioDescriptor {
    BUZZER_SCENARIO_CATALOG
        .iter()
        .find(|descriptor| descriptor.scenario == scenario)
        .expect("every CLI buzzer scenario has a catalogue descriptor")
}

fn prompt_menu_choice<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    prompt: &str,
    max: usize,
) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
    loop {
        write!(output, "{prompt} [1-{max}]: ")?;
        output.flush()?;
        let mut line = String::new();
        if input.read_line(&mut line)? == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "interactive buzzer selection ended before a choice was made",
            )
            .into());
        }
        if let Ok(choice) = line.trim().parse::<usize>()
            && (1..=max).contains(&choice)
        {
            return Ok(choice);
        }
        writeln!(output, "Enter a number from 1 through {max}.")?;
    }
}

fn buzzer_test_body(op: &str, cue: Option<&str>, scenario: Option<&str>, repeat: bool) -> Value {
    json!({
        "op": op,
        "cue": cue,
        "scenario": scenario,
        "repeat": repeat,
    })
}
