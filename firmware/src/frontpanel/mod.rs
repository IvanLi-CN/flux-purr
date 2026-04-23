use heapless::Vec;

pub mod render;

pub const FRONTPANEL_DEBOUNCE_MS: u64 = 20;
pub const FRONTPANEL_LONG_PRESS_MS: u64 = 500;
pub const FRONTPANEL_DOUBLE_CLICK_MS: u64 = 250;
pub const FRONTPANEL_PRESET_COUNT: usize = 10;
pub const FRONTPANEL_TARGET_TEMP_MIN_C: i16 = 0;
pub const FRONTPANEL_TARGET_TEMP_MAX_C: i16 = 400;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum RawFrontPanelKey {
    CenterBoot = 0,
    Right = 1,
    Down = 2,
    Left = 3,
    Up = 4,
}

impl RawFrontPanelKey {
    pub const ALL: [Self; 5] = [
        Self::CenterBoot,
        Self::Right,
        Self::Down,
        Self::Left,
        Self::Up,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::CenterBoot => "RAW CENTER",
            Self::Right => "RAW RIGHT",
            Self::Down => "RAW DOWN",
            Self::Left => "RAW LEFT",
            Self::Up => "RAW UP",
        }
    }

    pub const fn short_label(self) -> &'static str {
        match self {
            Self::CenterBoot => "CTR",
            Self::Right => "R",
            Self::Down => "D",
            Self::Left => "L",
            Self::Up => "U",
        }
    }

    pub const fn index(self) -> usize {
        self as usize
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FrontPanelKey {
    Center = 0,
    Right = 1,
    Down = 2,
    Left = 3,
    Up = 4,
}

impl FrontPanelKey {
    pub const ALL: [Self; 5] = [Self::Center, Self::Right, Self::Down, Self::Left, Self::Up];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Center => "CENTER",
            Self::Right => "RIGHT",
            Self::Down => "DOWN",
            Self::Left => "LEFT",
            Self::Up => "UP",
        }
    }

    pub const fn short_label(self) -> &'static str {
        match self {
            Self::Center => "CTR",
            Self::Right => "R",
            Self::Down => "D",
            Self::Left => "L",
            Self::Up => "U",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyGesture {
    ShortPress,
    DoublePress,
    LongPress,
}

impl KeyGesture {
    pub const fn label(self) -> &'static str {
        match self {
            Self::ShortPress => "SHORT",
            Self::DoublePress => "DOUBLE",
            Self::LongPress => "LONG",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyEvent {
    pub raw_key: RawFrontPanelKey,
    pub key: FrontPanelKey,
    pub gesture: KeyGesture,
    pub at_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrontPanelKeyMap {
    mapping: [FrontPanelKey; RawFrontPanelKey::ALL.len()],
}

impl Default for FrontPanelKeyMap {
    fn default() -> Self {
        Self {
            mapping: [
                FrontPanelKey::Center,
                FrontPanelKey::Right,
                FrontPanelKey::Left,
                FrontPanelKey::Down,
                FrontPanelKey::Up,
            ],
        }
    }
}

impl FrontPanelKeyMap {
    pub const fn with_mapping(mapping: [FrontPanelKey; RawFrontPanelKey::ALL.len()]) -> Self {
        Self { mapping }
    }

    pub const fn logical_from_raw(self, raw_key: RawFrontPanelKey) -> FrontPanelKey {
        self.mapping[raw_key.index()]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FrontPanelRawState {
    pressed_mask: u8,
}

impl FrontPanelRawState {
    pub const fn is_pressed(self, key: RawFrontPanelKey) -> bool {
        (self.pressed_mask & (1 << key.index())) != 0
    }

    pub fn set_pressed(&mut self, key: RawFrontPanelKey, pressed: bool) {
        let bit = 1 << key.index();
        if pressed {
            self.pressed_mask |= bit;
        } else {
            self.pressed_mask &= !bit;
        }
    }

    pub const fn pressed_mask(self) -> u8 {
        self.pressed_mask
    }

    pub fn first_pressed(self) -> Option<RawFrontPanelKey> {
        RawFrontPanelKey::ALL
            .iter()
            .copied()
            .find(|key| self.is_pressed(*key))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrontPanelInputTimings {
    pub debounce_ms: u64,
    pub long_press_ms: u64,
    pub double_click_ms: u64,
}

impl Default for FrontPanelInputTimings {
    fn default() -> Self {
        Self {
            debounce_ms: FRONTPANEL_DEBOUNCE_MS,
            long_press_ms: FRONTPANEL_LONG_PRESS_MS,
            double_click_ms: FRONTPANEL_DOUBLE_CLICK_MS,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct KeyTracker {
    raw_pressed: bool,
    stable_pressed: bool,
    last_raw_change_ms: u64,
    press_started_ms: Option<u64>,
    long_fired: bool,
    pending_short_release_ms: Option<u64>,
}

impl KeyTracker {
    const fn new() -> Self {
        Self {
            raw_pressed: false,
            stable_pressed: false,
            last_raw_change_ms: 0,
            press_started_ms: None,
            long_fired: false,
            pending_short_release_ms: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrontPanelSampleResult {
    pub raw_state: FrontPanelRawState,
    pub events: Vec<KeyEvent, 10>,
}

impl FrontPanelSampleResult {
    fn new(raw_state: FrontPanelRawState) -> Self {
        Self {
            raw_state,
            events: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrontPanelInputController {
    key_map: FrontPanelKeyMap,
    timings: FrontPanelInputTimings,
    trackers: [KeyTracker; RawFrontPanelKey::ALL.len()],
}

impl Default for FrontPanelInputController {
    fn default() -> Self {
        Self::new(
            FrontPanelKeyMap::default(),
            FrontPanelInputTimings::default(),
        )
    }
}

impl FrontPanelInputController {
    pub const fn new(key_map: FrontPanelKeyMap, timings: FrontPanelInputTimings) -> Self {
        Self {
            key_map,
            timings,
            trackers: [KeyTracker::new(); RawFrontPanelKey::ALL.len()],
        }
    }

    pub fn sample(&mut self, now_ms: u64, raw_state: FrontPanelRawState) -> FrontPanelSampleResult {
        let mut result = FrontPanelSampleResult::new(raw_state);

        for raw_key in RawFrontPanelKey::ALL {
            let logical_key = self.key_map.logical_from_raw(raw_key);
            let pressed = raw_state.is_pressed(raw_key);
            let tracker = &mut self.trackers[raw_key.index()];

            if pressed != tracker.raw_pressed {
                tracker.raw_pressed = pressed;
                tracker.last_raw_change_ms = now_ms;

                if pressed {
                    tracker.press_started_ms = Some(now_ms);
                    tracker.long_fired = false;
                } else if !tracker.stable_pressed {
                    tracker.press_started_ms = None;
                    tracker.long_fired = false;
                }
            }

            if tracker.pending_short_release_ms.is_some_and(|released| {
                now_ms.saturating_sub(released) > self.timings.double_click_ms
            }) {
                tracker.pending_short_release_ms = None;
                let _ = result.events.push(KeyEvent {
                    raw_key,
                    key: logical_key,
                    gesture: KeyGesture::ShortPress,
                    at_ms: now_ms,
                });
            }

            if tracker.raw_pressed != tracker.stable_pressed
                && now_ms.saturating_sub(tracker.last_raw_change_ms) >= self.timings.debounce_ms
            {
                tracker.stable_pressed = tracker.raw_pressed;
                if tracker.stable_pressed {
                    tracker.long_fired = false;
                } else if tracker.press_started_ms.take().is_some() && !tracker.long_fired {
                    if let Some(previous_release_ms) = tracker.pending_short_release_ms {
                        if now_ms.saturating_sub(previous_release_ms)
                            <= self.timings.double_click_ms
                        {
                            tracker.pending_short_release_ms = None;
                            let _ = result.events.push(KeyEvent {
                                raw_key,
                                key: logical_key,
                                gesture: KeyGesture::DoublePress,
                                at_ms: now_ms,
                            });
                        } else {
                            tracker.pending_short_release_ms = Some(now_ms);
                        }
                    } else {
                        tracker.pending_short_release_ms = Some(now_ms);
                    }
                }
            }

            if tracker.stable_pressed
                && !tracker.long_fired
                && tracker.press_started_ms.is_some_and(|started| {
                    now_ms.saturating_sub(started) >= self.timings.long_press_ms
                })
            {
                tracker.long_fired = true;
                tracker.pending_short_release_ms = None;
                let _ = result.events.push(KeyEvent {
                    raw_key,
                    key: logical_key,
                    gesture: KeyGesture::LongPress,
                    at_ms: now_ms,
                });
            }
        }

        result
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrontPanelRuntimeMode {
    KeyTest,
    App,
}

impl FrontPanelRuntimeMode {
    pub const fn compile_time_default() -> Self {
        if cfg!(feature = "frontpanel-key-test") {
            Self::KeyTest
        } else {
            Self::App
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrontPanelRoute {
    KeyTest,
    Dashboard,
    Menu,
    PresetTemp,
    ActiveCooling,
    WifiInfo,
    DeviceInfo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrontPanelMenuItem {
    PresetTemp,
    ActiveCooling,
    WifiInfo,
    DeviceInfo,
}

impl FrontPanelMenuItem {
    pub const ALL: [Self; 4] = [
        Self::PresetTemp,
        Self::ActiveCooling,
        Self::WifiInfo,
        Self::DeviceInfo,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::PresetTemp => "Preset Temp",
            Self::ActiveCooling => "Active Cooling",
            Self::WifiInfo => "WiFi Info",
            Self::DeviceInfo => "Device Info",
        }
    }

    pub const fn short_label(self) -> &'static str {
        match self {
            Self::PresetTemp => "TEMP",
            Self::ActiveCooling => "COOL",
            Self::WifiInfo => "WIFI",
            Self::DeviceInfo => "INFO",
        }
    }

    pub const fn route(self) -> FrontPanelRoute {
        match self {
            Self::PresetTemp => FrontPanelRoute::PresetTemp,
            Self::ActiveCooling => FrontPanelRoute::ActiveCooling,
            Self::WifiInfo => FrontPanelRoute::WifiInfo,
            Self::DeviceInfo => FrontPanelRoute::DeviceInfo,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FanDisplayState {
    Off,
    Auto,
    Run,
}

impl FanDisplayState {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Off => "OFF",
            Self::Auto => "AUTO",
            Self::Run => "RUN",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaterLockReason {
    CoolingDisabledOvertemp,
    HardOvertemp,
}

impl HeaterLockReason {
    pub const fn label(self) -> &'static str {
        match self {
            Self::CoolingDisabledOvertemp => "cooling-disabled-overtemp",
            Self::HardOvertemp => "hard-overtemp",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct KeyTestState {
    pub raw_state: FrontPanelRawState,
    pub last_raw_key: Option<RawFrontPanelKey>,
    pub last_key: Option<FrontPanelKey>,
    pub last_gesture: Option<KeyGesture>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrontPanelUiState {
    pub runtime_mode: FrontPanelRuntimeMode,
    pub route: FrontPanelRoute,
    pub current_temp_c: i16,
    pub current_temp_deci_c: i16,
    pub pd_contract_mv: u16,
    pub target_temp_c: i16,
    pub heater_enabled: bool,
    pub heater_output_percent: u8,
    pub fan_enabled: bool,
    pub fan_display_state: FanDisplayState,
    pub heater_lock_reason: Option<HeaterLockReason>,
    pub dashboard_warning_visible: bool,
    pub selected_menu_item: FrontPanelMenuItem,
    pub selected_preset_slot: usize,
    pub presets_c: [Option<i16>; FRONTPANEL_PRESET_COUNT],
    pub active_cooling_enabled: bool,
    pub key_test: KeyTestState,
}

impl FrontPanelUiState {
    pub fn new(runtime_mode: FrontPanelRuntimeMode) -> Self {
        let route = match runtime_mode {
            FrontPanelRuntimeMode::KeyTest => FrontPanelRoute::KeyTest,
            FrontPanelRuntimeMode::App => FrontPanelRoute::Dashboard,
        };

        Self {
            runtime_mode,
            route,
            current_temp_c: 300,
            current_temp_deci_c: 3000,
            pd_contract_mv: crate::DEFAULT_PD_VOLTAGE_REQUEST.millivolts(),
            target_temp_c: 100,
            heater_enabled: false,
            heater_output_percent: 0,
            fan_enabled: false,
            fan_display_state: FanDisplayState::Auto,
            heater_lock_reason: None,
            dashboard_warning_visible: false,
            selected_menu_item: FrontPanelMenuItem::ActiveCooling,
            selected_preset_slot: 1,
            presets_c: [
                Some(50),
                Some(100),
                Some(120),
                Some(150),
                Some(180),
                Some(200),
                Some(210),
                Some(220),
                Some(250),
                Some(300),
            ],
            active_cooling_enabled: true,
            key_test: KeyTestState::default(),
        }
    }

    pub fn set_raw_state(&mut self, raw_state: FrontPanelRawState) {
        self.key_test.raw_state = raw_state;
        if let Some(raw_key) = raw_state.first_pressed() {
            self.key_test.last_raw_key = Some(raw_key);
        }
    }

    pub fn handle_event(&mut self, event: KeyEvent) -> bool {
        self.key_test.last_raw_key = Some(event.raw_key);
        self.key_test.last_key = Some(event.key);
        self.key_test.last_gesture = Some(event.gesture);

        match self.runtime_mode {
            FrontPanelRuntimeMode::KeyTest => true,
            FrontPanelRuntimeMode::App => self.apply_app_event(event),
        }
    }

    pub fn selected_preset(&self) -> Option<i16> {
        self.presets_c[self.selected_preset_slot]
    }

    pub fn matching_preset_slot(&self) -> Option<usize> {
        if self.selected_preset() == Some(self.target_temp_c) {
            return Some(self.selected_preset_slot);
        }

        self.presets_c
            .iter()
            .position(|preset| preset.is_some_and(|value| value == self.target_temp_c))
    }

    pub fn ensure_selected_preset_slot(&mut self) {
        if self.selected_preset_slot >= FRONTPANEL_PRESET_COUNT {
            self.selected_preset_slot = 0;
        }
    }

    pub fn set_target_temp_c(&mut self, target_temp_c: i16) {
        self.target_temp_c = clamp_target_temp_c(target_temp_c);
        self.selected_preset_slot = self
            .matching_preset_slot()
            .unwrap_or(self.selected_preset_slot);
    }

    fn apply_app_event(&mut self, event: KeyEvent) -> bool {
        match self.route {
            FrontPanelRoute::KeyTest => {
                self.route = FrontPanelRoute::Dashboard;
                true
            }
            FrontPanelRoute::Dashboard => self.apply_dashboard_event(event),
            FrontPanelRoute::Menu => self.apply_menu_event(event),
            FrontPanelRoute::PresetTemp => self.apply_preset_temp_event(event),
            FrontPanelRoute::ActiveCooling => self.apply_active_cooling_event(event),
            FrontPanelRoute::WifiInfo | FrontPanelRoute::DeviceInfo => {
                self.apply_readonly_page_event(event)
            }
        }
    }

    fn apply_dashboard_event(&mut self, event: KeyEvent) -> bool {
        match (event.key, event.gesture) {
            (FrontPanelKey::Up, KeyGesture::ShortPress) => {
                self.set_target_temp_c(self.target_temp_c.saturating_add(1));
                true
            }
            (FrontPanelKey::Down, KeyGesture::ShortPress) => {
                self.set_target_temp_c(self.target_temp_c.saturating_sub(1));
                true
            }
            (FrontPanelKey::Left, KeyGesture::ShortPress) => {
                if let Some((slot, temp)) = self.find_neighbor_preset(false) {
                    self.selected_preset_slot = slot;
                    self.set_target_temp_c(temp);
                    true
                } else {
                    false
                }
            }
            (FrontPanelKey::Right, KeyGesture::ShortPress) => {
                if let Some((slot, temp)) = self.find_neighbor_preset(true) {
                    self.selected_preset_slot = slot;
                    self.set_target_temp_c(temp);
                    true
                } else {
                    false
                }
            }
            (FrontPanelKey::Center, KeyGesture::ShortPress) => {
                self.heater_enabled = !self.heater_enabled;
                true
            }
            (FrontPanelKey::Center, KeyGesture::DoublePress) => {
                self.active_cooling_enabled = !self.active_cooling_enabled;
                true
            }
            (FrontPanelKey::Center, KeyGesture::LongPress) => {
                self.route = FrontPanelRoute::Menu;
                true
            }
            _ => false,
        }
    }

    fn apply_menu_event(&mut self, event: KeyEvent) -> bool {
        match (event.key, event.gesture) {
            (FrontPanelKey::Left, KeyGesture::ShortPress) => {
                self.selected_menu_item = match self.selected_menu_item {
                    FrontPanelMenuItem::PresetTemp => FrontPanelMenuItem::DeviceInfo,
                    FrontPanelMenuItem::ActiveCooling => FrontPanelMenuItem::PresetTemp,
                    FrontPanelMenuItem::WifiInfo => FrontPanelMenuItem::ActiveCooling,
                    FrontPanelMenuItem::DeviceInfo => FrontPanelMenuItem::WifiInfo,
                };
                true
            }
            (FrontPanelKey::Right, KeyGesture::ShortPress) => {
                self.selected_menu_item = match self.selected_menu_item {
                    FrontPanelMenuItem::PresetTemp => FrontPanelMenuItem::ActiveCooling,
                    FrontPanelMenuItem::ActiveCooling => FrontPanelMenuItem::WifiInfo,
                    FrontPanelMenuItem::WifiInfo => FrontPanelMenuItem::DeviceInfo,
                    FrontPanelMenuItem::DeviceInfo => FrontPanelMenuItem::PresetTemp,
                };
                true
            }
            (FrontPanelKey::Center, KeyGesture::ShortPress) => {
                self.route = self.selected_menu_item.route();
                if self.route == FrontPanelRoute::PresetTemp {
                    self.ensure_selected_preset_slot();
                }
                true
            }
            (FrontPanelKey::Center, KeyGesture::LongPress) => {
                self.route = FrontPanelRoute::Dashboard;
                true
            }
            _ => false,
        }
    }

    fn apply_preset_temp_event(&mut self, event: KeyEvent) -> bool {
        match (event.key, event.gesture) {
            (FrontPanelKey::Right, KeyGesture::ShortPress) => {
                self.selected_preset_slot = self.advance_preset_slot();
                true
            }
            (FrontPanelKey::Up, KeyGesture::ShortPress) => {
                self.ensure_selected_preset_slot();
                let next_temp = self.presets_c[self.selected_preset_slot]
                    .map(|temp| temp.saturating_add(1))
                    .unwrap_or(FRONTPANEL_TARGET_TEMP_MIN_C);
                let next_temp = clamp_target_temp_c(next_temp);
                self.presets_c[self.selected_preset_slot] = Some(next_temp);
                self.set_target_temp_c(next_temp);
                true
            }
            (FrontPanelKey::Down, KeyGesture::ShortPress) => {
                self.ensure_selected_preset_slot();
                match self.presets_c[self.selected_preset_slot] {
                    Some(temp) if temp > FRONTPANEL_TARGET_TEMP_MIN_C => {
                        let next_temp = clamp_target_temp_c(temp.saturating_sub(1));
                        self.presets_c[self.selected_preset_slot] = Some(next_temp);
                        self.set_target_temp_c(next_temp);
                    }
                    Some(_) | None => {
                        self.presets_c[self.selected_preset_slot] = None;
                    }
                }
                true
            }
            (FrontPanelKey::Left, KeyGesture::ShortPress)
            | (FrontPanelKey::Center, KeyGesture::ShortPress)
            | (FrontPanelKey::Center, KeyGesture::LongPress) => {
                self.route = FrontPanelRoute::Menu;
                true
            }
            _ => false,
        }
    }

    fn apply_active_cooling_event(&mut self, event: KeyEvent) -> bool {
        match (event.key, event.gesture) {
            (FrontPanelKey::Left, KeyGesture::ShortPress)
            | (FrontPanelKey::Center, KeyGesture::ShortPress)
            | (FrontPanelKey::Center, KeyGesture::LongPress) => {
                self.route = FrontPanelRoute::Menu;
                true
            }
            _ => false,
        }
    }

    fn apply_readonly_page_event(&mut self, event: KeyEvent) -> bool {
        match (event.key, event.gesture) {
            (FrontPanelKey::Left, KeyGesture::ShortPress)
            | (FrontPanelKey::Center, KeyGesture::ShortPress)
            | (FrontPanelKey::Center, KeyGesture::LongPress) => {
                self.route = FrontPanelRoute::Menu;
                true
            }
            _ => false,
        }
    }

    fn sorted_active_presets(&self) -> Vec<(usize, i16), FRONTPANEL_PRESET_COUNT> {
        let mut sorted: Vec<(usize, i16), FRONTPANEL_PRESET_COUNT> = Vec::new();
        for (slot, preset) in self.presets_c.iter().enumerate() {
            if let Some(temp) = preset {
                let _ = sorted.push((slot, *temp));
            }
        }
        sorted.sort_unstable_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
        sorted
    }

    fn find_neighbor_preset(&self, ascending: bool) -> Option<(usize, i16)> {
        let sorted = self.sorted_active_presets();
        if ascending {
            sorted
                .iter()
                .copied()
                .find(|(_, temp)| *temp > self.target_temp_c)
        } else {
            sorted
                .iter()
                .copied()
                .rev()
                .find(|(_, temp)| *temp < self.target_temp_c)
        }
    }

    fn advance_preset_slot(&self) -> usize {
        (self.selected_preset_slot + 1) % FRONTPANEL_PRESET_COUNT
    }
}

fn clamp_target_temp_c(target_temp_c: i16) -> i16 {
    target_temp_c.clamp(FRONTPANEL_TARGET_TEMP_MIN_C, FRONTPANEL_TARGET_TEMP_MAX_C)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw_state(keys: &[RawFrontPanelKey]) -> FrontPanelRawState {
        let mut state = FrontPanelRawState::default();
        for key in keys {
            state.set_pressed(*key, true);
        }
        state
    }

    fn collect_events(
        controller: &mut FrontPanelInputController,
        samples: &[(u64, FrontPanelRawState)],
    ) -> Vec<KeyEvent, 16> {
        let mut events = Vec::new();
        for (timestamp_ms, state) in samples {
            let result = controller.sample(*timestamp_ms, *state);
            for event in result.events {
                let _ = events.push(event);
            }
        }
        events
    }

    #[test]
    fn short_press_waits_for_double_click_window() {
        let mut controller = FrontPanelInputController::default();
        let events = collect_events(
            &mut controller,
            &[
                (0, raw_state(&[])),
                (5, raw_state(&[RawFrontPanelKey::Up])),
                (30, raw_state(&[RawFrontPanelKey::Up])),
                (35, raw_state(&[])),
                (65, raw_state(&[])),
                (330, raw_state(&[])),
            ],
        );

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].key, FrontPanelKey::Up);
        assert_eq!(events[0].gesture, KeyGesture::ShortPress);
    }

    #[test]
    fn debounce_filters_bounce_before_emitting_short_press() {
        let mut controller = FrontPanelInputController::default();
        let events = collect_events(
            &mut controller,
            &[
                (0, raw_state(&[])),
                (5, raw_state(&[RawFrontPanelKey::Left])),
                (15, raw_state(&[])),
                (20, raw_state(&[RawFrontPanelKey::Left])),
                (50, raw_state(&[RawFrontPanelKey::Left])),
                (60, raw_state(&[])),
                (100, raw_state(&[])),
                (360, raw_state(&[])),
            ],
        );

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].key, FrontPanelKey::Down);
        assert_eq!(events[0].gesture, KeyGesture::ShortPress);
    }

    #[test]
    fn exact_twenty_ms_press_survives_debounce() {
        let mut controller = FrontPanelInputController::default();
        let events = collect_events(
            &mut controller,
            &[
                (0, raw_state(&[])),
                (5, raw_state(&[RawFrontPanelKey::Up])),
                (25, raw_state(&[RawFrontPanelKey::Up])),
                (35, raw_state(&[])),
                (55, raw_state(&[])),
                (320, raw_state(&[])),
            ],
        );

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].key, FrontPanelKey::Up);
        assert_eq!(events[0].gesture, KeyGesture::ShortPress);
    }

    #[test]
    fn double_press_emits_single_double_event() {
        let mut controller = FrontPanelInputController::default();
        let events = collect_events(
            &mut controller,
            &[
                (0, raw_state(&[])),
                (10, raw_state(&[RawFrontPanelKey::CenterBoot])),
                (40, raw_state(&[RawFrontPanelKey::CenterBoot])),
                (50, raw_state(&[])),
                (80, raw_state(&[])),
                (120, raw_state(&[RawFrontPanelKey::CenterBoot])),
                (150, raw_state(&[RawFrontPanelKey::CenterBoot])),
                (160, raw_state(&[])),
                (200, raw_state(&[])),
            ],
        );

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].key, FrontPanelKey::Center);
        assert_eq!(events[0].gesture, KeyGesture::DoublePress);
    }

    #[test]
    fn long_press_emits_once_and_does_not_backfill_short_press() {
        let mut controller = FrontPanelInputController::default();
        let events = collect_events(
            &mut controller,
            &[
                (0, raw_state(&[])),
                (10, raw_state(&[RawFrontPanelKey::Right])),
                (40, raw_state(&[RawFrontPanelKey::Right])),
                (550, raw_state(&[RawFrontPanelKey::Right])),
                (700, raw_state(&[RawFrontPanelKey::Right])),
                (710, raw_state(&[])),
                (1_000, raw_state(&[])),
            ],
        );

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].key, FrontPanelKey::Right);
        assert_eq!(events[0].gesture, KeyGesture::LongPress);
    }

    #[test]
    fn long_press_uses_the_raw_press_edge() {
        let mut controller = FrontPanelInputController::default();
        let events = collect_events(
            &mut controller,
            &[
                (0, raw_state(&[])),
                (10, raw_state(&[RawFrontPanelKey::Right])),
                (30, raw_state(&[RawFrontPanelKey::Right])),
                (509, raw_state(&[RawFrontPanelKey::Right])),
                (510, raw_state(&[RawFrontPanelKey::Right])),
            ],
        );

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].key, FrontPanelKey::Right);
        assert_eq!(events[0].gesture, KeyGesture::LongPress);
        assert_eq!(events[0].at_ms, 510);
    }

    #[test]
    fn different_keys_do_not_cross_contaminate_pending_clicks() {
        let mut controller = FrontPanelInputController::default();
        let events = collect_events(
            &mut controller,
            &[
                (0, raw_state(&[])),
                (10, raw_state(&[RawFrontPanelKey::Left])),
                (40, raw_state(&[RawFrontPanelKey::Left])),
                (50, raw_state(&[])),
                (100, raw_state(&[RawFrontPanelKey::Up])),
                (130, raw_state(&[RawFrontPanelKey::Up])),
                (140, raw_state(&[])),
                (330, raw_state(&[])),
                (450, raw_state(&[])),
                (620, raw_state(&[])),
            ],
        );

        assert_eq!(events.len(), 2);
        assert_eq!(events[0].key, FrontPanelKey::Down);
        assert_eq!(events[0].gesture, KeyGesture::ShortPress);
        assert_eq!(events[1].key, FrontPanelKey::Up);
        assert_eq!(events[1].gesture, KeyGesture::ShortPress);
    }

    #[test]
    fn slow_second_press_preserves_the_first_short_press() {
        let mut controller = FrontPanelInputController::default();
        let events = collect_events(
            &mut controller,
            &[
                (0, raw_state(&[])),
                (10, raw_state(&[RawFrontPanelKey::Up])),
                (40, raw_state(&[RawFrontPanelKey::Up])),
                (50, raw_state(&[])),
                (80, raw_state(&[])),
                (300, raw_state(&[RawFrontPanelKey::Up])),
                (325, raw_state(&[RawFrontPanelKey::Up])),
                (335, raw_state(&[])),
                (360, raw_state(&[])),
                (620, raw_state(&[])),
            ],
        );

        assert_eq!(events.len(), 2);
        assert_eq!(events[0].key, FrontPanelKey::Up);
        assert_eq!(events[0].gesture, KeyGesture::ShortPress);
        assert_eq!(events[1].key, FrontPanelKey::Up);
        assert_eq!(events[1].gesture, KeyGesture::ShortPress);
    }

    #[test]
    fn default_presets_match_the_calibrated_temperature_ladder() {
        let state = FrontPanelUiState::new(FrontPanelRuntimeMode::App);
        assert_eq!(
            state.presets_c,
            [
                Some(50),
                Some(100),
                Some(120),
                Some(150),
                Some(180),
                Some(200),
                Some(210),
                Some(220),
                Some(250),
                Some(300),
            ]
        );
        assert_eq!(state.selected_preset_slot, 1);
        assert_eq!(
            state.pd_contract_mv,
            crate::DEFAULT_PD_VOLTAGE_REQUEST.millivolts()
        );
        assert_eq!(state.fan_display_state, FanDisplayState::Auto);
        assert_eq!(state.heater_lock_reason, None);
    }

    #[test]
    fn dashboard_navigation_uses_sorted_preset_temperatures() {
        let mut state = FrontPanelUiState::new(FrontPanelRuntimeMode::App);
        state.target_temp_c = 189;

        assert!(state.handle_event(KeyEvent {
            raw_key: RawFrontPanelKey::Left,
            key: FrontPanelKey::Left,
            gesture: KeyGesture::ShortPress,
            at_ms: 0,
        }));
        assert_eq!(state.target_temp_c, 180);

        assert!(state.handle_event(KeyEvent {
            raw_key: RawFrontPanelKey::Right,
            key: FrontPanelKey::Right,
            gesture: KeyGesture::ShortPress,
            at_ms: 0,
        }));
        assert_eq!(state.target_temp_c, 200);
    }

    #[test]
    fn dashboard_center_short_toggles_heater_and_double_press_toggles_cooling_policy() {
        let mut state = FrontPanelUiState::new(FrontPanelRuntimeMode::App);

        assert!(state.handle_event(KeyEvent {
            raw_key: RawFrontPanelKey::CenterBoot,
            key: FrontPanelKey::Center,
            gesture: KeyGesture::ShortPress,
            at_ms: 0,
        }));
        assert!(state.heater_enabled);
        assert_eq!(state.route, FrontPanelRoute::Dashboard);

        assert!(state.handle_event(KeyEvent {
            raw_key: RawFrontPanelKey::CenterBoot,
            key: FrontPanelKey::Center,
            gesture: KeyGesture::DoublePress,
            at_ms: 0,
        }));
        assert!(!state.fan_enabled);
        assert!(!state.active_cooling_enabled);
        assert_eq!(state.fan_display_state, FanDisplayState::Auto);
        assert_eq!(state.route, FrontPanelRoute::Dashboard);

        assert!(state.handle_event(KeyEvent {
            raw_key: RawFrontPanelKey::CenterBoot,
            key: FrontPanelKey::Center,
            gesture: KeyGesture::LongPress,
            at_ms: 0,
        }));
        assert_eq!(state.route, FrontPanelRoute::Menu);
    }

    #[test]
    fn child_pages_exit_back_to_menu() {
        let mut state = FrontPanelUiState::new(FrontPanelRuntimeMode::App);
        state.route = FrontPanelRoute::Menu;
        state.selected_menu_item = FrontPanelMenuItem::PresetTemp;
        assert!(state.handle_event(KeyEvent {
            raw_key: RawFrontPanelKey::CenterBoot,
            key: FrontPanelKey::Center,
            gesture: KeyGesture::ShortPress,
            at_ms: 0,
        }));
        assert_eq!(state.route, FrontPanelRoute::PresetTemp);

        assert!(state.handle_event(KeyEvent {
            raw_key: RawFrontPanelKey::CenterBoot,
            key: FrontPanelKey::Center,
            gesture: KeyGesture::LongPress,
            at_ms: 0,
        }));
        assert_eq!(state.route, FrontPanelRoute::Menu);
    }

    #[test]
    fn preset_temp_navigation_cycles_through_disabled_slots() {
        let mut state = FrontPanelUiState::new(FrontPanelRuntimeMode::App);
        state.route = FrontPanelRoute::PresetTemp;
        state.selected_preset_slot = 1;
        state.presets_c[2] = None;

        assert!(state.handle_event(KeyEvent {
            raw_key: RawFrontPanelKey::Right,
            key: FrontPanelKey::Right,
            gesture: KeyGesture::ShortPress,
            at_ms: 0,
        }));
        assert_eq!(state.selected_preset_slot, 2);
        assert_eq!(state.selected_preset(), None);
    }

    #[test]
    fn preset_temp_can_cross_between_zero_and_disabled_state() {
        let mut state = FrontPanelUiState::new(FrontPanelRuntimeMode::App);
        state.route = FrontPanelRoute::PresetTemp;
        state.selected_preset_slot = 2;
        state.presets_c[2] = None;

        assert!(state.handle_event(KeyEvent {
            raw_key: RawFrontPanelKey::Up,
            key: FrontPanelKey::Up,
            gesture: KeyGesture::ShortPress,
            at_ms: 0,
        }));
        assert_eq!(state.presets_c[2], Some(0));
        assert_eq!(state.target_temp_c, 0);

        assert!(state.handle_event(KeyEvent {
            raw_key: RawFrontPanelKey::Down,
            key: FrontPanelKey::Down,
            gesture: KeyGesture::ShortPress,
            at_ms: 0,
        }));
        assert_eq!(state.presets_c[2], None);
        assert_eq!(state.target_temp_c, 0);
    }

    #[test]
    fn matching_preset_slot_prefers_the_current_duplicate_slot() {
        let mut state = FrontPanelUiState::new(FrontPanelRuntimeMode::App);
        state.selected_preset_slot = 4;
        state.presets_c[2] = Some(200);
        state.presets_c[4] = Some(200);
        state.target_temp_c = 199;

        assert!(state.handle_event(KeyEvent {
            raw_key: RawFrontPanelKey::Up,
            key: FrontPanelKey::Up,
            gesture: KeyGesture::ShortPress,
            at_ms: 0,
        }));
        assert_eq!(state.target_temp_c, 200);
        assert_eq!(state.selected_preset_slot, 4);
    }

    #[test]
    fn dashboard_target_temp_clamps_to_working_range() {
        let mut state = FrontPanelUiState::new(FrontPanelRuntimeMode::App);
        state.set_target_temp_c(-10);
        assert_eq!(state.target_temp_c, FRONTPANEL_TARGET_TEMP_MIN_C);

        state.set_target_temp_c(450);
        assert_eq!(state.target_temp_c, FRONTPANEL_TARGET_TEMP_MAX_C);
    }

    #[test]
    fn preset_temp_editing_clamps_to_working_range() {
        let mut state = FrontPanelUiState::new(FrontPanelRuntimeMode::App);
        state.route = FrontPanelRoute::PresetTemp;
        state.selected_preset_slot = 3;
        state.presets_c[3] = Some(FRONTPANEL_TARGET_TEMP_MAX_C);

        assert!(state.handle_event(KeyEvent {
            raw_key: RawFrontPanelKey::Up,
            key: FrontPanelKey::Up,
            gesture: KeyGesture::ShortPress,
            at_ms: 0,
        }));
        assert_eq!(state.presets_c[3], Some(FRONTPANEL_TARGET_TEMP_MAX_C));
        assert_eq!(state.target_temp_c, FRONTPANEL_TARGET_TEMP_MAX_C);
    }

    #[test]
    fn key_test_release_keeps_the_last_raw_label() {
        let mut state = FrontPanelUiState::new(FrontPanelRuntimeMode::KeyTest);
        state.set_raw_state(raw_state(&[RawFrontPanelKey::Down]));
        assert!(state.handle_event(KeyEvent {
            raw_key: RawFrontPanelKey::Down,
            key: FrontPanelKey::Left,
            gesture: KeyGesture::LongPress,
            at_ms: 0,
        }));

        state.set_raw_state(raw_state(&[]));
        assert_eq!(state.key_test.last_raw_key, Some(RawFrontPanelKey::Down));
        assert_eq!(state.key_test.last_key, Some(FrontPanelKey::Left));
        assert_eq!(state.key_test.last_gesture, Some(KeyGesture::LongPress));
    }

    #[test]
    fn key_test_mode_updates_diagnostics_without_leaving_route() {
        let mut state = FrontPanelUiState::new(FrontPanelRuntimeMode::KeyTest);
        state.set_raw_state(raw_state(&[RawFrontPanelKey::Down]));
        assert!(state.handle_event(KeyEvent {
            raw_key: RawFrontPanelKey::Down,
            key: FrontPanelKey::Left,
            gesture: KeyGesture::LongPress,
            at_ms: 0,
        }));
        assert_eq!(state.route, FrontPanelRoute::KeyTest);
        assert_eq!(state.key_test.last_raw_key, Some(RawFrontPanelKey::Down));
        assert_eq!(state.key_test.last_key, Some(FrontPanelKey::Left));
        assert_eq!(state.key_test.last_gesture, Some(KeyGesture::LongPress));
    }
}
