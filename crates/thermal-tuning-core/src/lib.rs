#![no_std]

#[cfg(test)]
extern crate std;

use sha2::{Digest, Sha256};

pub const CORE_SCHEMA: &str = "thermal_tuning_core_v1";
pub const TARGET_COUNT: usize = 9;
pub const TRACE_EVENT_CAPACITY: usize = 96;
// One target temperature plus every point-local fixed-point control parameter.
pub const CANDIDATE_POINT_CANONICAL_BYTES: usize = 2 + (19 * 2);
pub const CANDIDATE_PROFILE_CANONICAL_BYTES: usize =
    1 + TARGET_COUNT * CANDIDATE_POINT_CANONICAL_BYTES;
pub const TARGET_BUDGET_SECONDS: u32 = 20 * 60;
pub const HOLD_CONFIRM_SECONDS: u32 = 60;
/// A short local warmup must still settle promptly. Longer ramps receive a
/// deterministic allowance proportional to their measured starting delta.
/// Start the 60 second verification only after entering a narrower band than
/// the acceptance band, leaving room for normal plant transport variation.
pub const HOLD_CONFIRM_ENTRY_CENTI: i16 = 200;
pub const MAX_OVERSHOOT_CENTI: i16 = 300;
pub const MAX_HOLD_PEAK_TO_PEAK_CENTI: i16 = 300;
pub const CANDIDATE_LADDER_WIDTH: usize = 3;

pub const GATE_WARMUP_COMPLETE: u16 = 1 << 0;
pub const GATE_STAGE_COMPLETE: u16 = 1 << 1;
pub const GATE_OVERSHOOT: u16 = 1 << 2;
pub const GATE_HOLD_PEAK_TO_PEAK: u16 = 1 << 3;
pub const GATE_HOLD_CONFIRM: u16 = 1 << 4;

pub const PHYSICAL_TARGETS_C: [i16; TARGET_COUNT] = [60, 80, 100, 120, 140, 160, 180, 220, 240];
pub const EXECUTION_ORDER_C: [i16; TARGET_COUNT] = [60, 240, 140, 100, 80, 120, 180, 160, 220];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PpsPowerClass {
    Pps3a,
    Pps5a,
}

impl PpsPowerClass {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pps3a => "pps3a",
            Self::Pps5a => "pps5a",
        }
    }

    pub const fn nominal_contract(self) -> (u16, u16) {
        match self {
            // 65 W belongs to the 3 A class: 20 V at 3.25 A.
            Self::Pps3a => (20_000, 3_250),
            Self::Pps5a => (20_000, 5_000),
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "pps3a" => Some(Self::Pps3a),
            "pps5a" => Some(Self::Pps5a),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunState {
    Idle,
    Running,
    Terminal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Idle,
    CooldownWait,
    Scout,
    Retune,
    HoldConfirm,
    Terminal,
}

impl Phase {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::CooldownWait => "cooldown_wait",
            Self::Scout => "scout",
            Self::Retune => "retune",
            Self::HoldConfirm => "hold_confirm",
            Self::Terminal => "terminal",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalDisposition {
    Completed,
    Failed,
    Cancelled,
    BudgetExhausted,
    SafetyDisarmed,
    ReviewIncomplete,
    InterruptedReset,
}

impl TerminalDisposition {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::BudgetExhausted => "budget_exhausted",
            Self::SafetyDisarmed => "safety_disarmed",
            Self::ReviewIncomplete => "review_incomplete",
            Self::InterruptedReset => "interrupted_reset",
        }
    }

    pub const fn is_success(self) -> bool {
        matches!(self, Self::Completed)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromotionState {
    Unavailable,
    AwaitingReview,
    Ready,
    Previewed,
    Saved,
    Expired,
}

impl PromotionState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unavailable => "unavailable",
            Self::AwaitingReview => "awaiting_review",
            Self::Ready => "ready",
            Self::Previewed => "previewed",
            Self::Saved => "saved",
            Self::Expired => "expired",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetDisposition {
    Pending,
    Accepted,
    Failed,
    Skipped,
}

impl TargetDisposition {
    const fn as_byte(self) -> u8 {
        match self {
            Self::Pending => 0,
            Self::Accepted => 1,
            Self::Failed => 2,
            Self::Skipped => 3,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Eligibility {
    pub thermal_model_ready: bool,
    pub curve_covers_all_targets: bool,
    pub pps_class_available: bool,
    pub idle: bool,
    pub measurement_safe: bool,
}

impl Eligibility {
    pub const fn ready(self) -> bool {
        self.thermal_model_ready
            && self.curve_covers_all_targets
            && self.pps_class_available
            && self.idle
            && self.measurement_safe
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartError {
    UnsupportedPowerClass,
    Ineligible,
    Busy,
    InvalidTraceCapacity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraceError {
    NotRunning,
    Gap,
    DigestMismatch,
    Range,
    NotTerminal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromotionError {
    Unavailable,
    Mismatch,
    ReviewIncomplete,
    NotPreviewed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CandidatePoint {
    pub target_c: i16,
    pub brake_distance_centi_c: u16,
    pub warmup_power_permille: u16,
    pub warmup_reenter_centi_c: u16,
    pub approach_power_permille: u16,
    pub approach_floor_power_permille: u16,
    pub approach_damping_exponent_permille: u16,
    pub approach_tail_window_centi_c: u16,
    pub hold_power_permille: u16,
    pub hold_reheat_power_permille: u16,
    pub hold_entry_centi_c: u16,
    pub hold_exit_centi_c: u16,
    pub hold_on_centi_c: u16,
    pub hold_off_centi_c: u16,
    pub overshoot_cutoff_centi_c: u16,
    pub hold_kp_permille_per_c: u16,
    pub hold_ki_permille_per_c_tick: u16,
    pub hold_blend_ticks: u16,
    pub approach_lead_ticks: u16,
    pub hold_lead_ticks: u16,
}

impl CandidatePoint {
    pub const fn baseline(target_c: i16, class: PpsPowerClass) -> Self {
        let high_power = matches!(class, PpsPowerClass::Pps5a);
        let high_temperature = high_power && target_c >= 200;
        // The measured 5 A low-temperature plant retains substantial energy
        // after its warmup phase. Begin braking early and make the full-speed
        // portion itself bounded; the ladder below then brackets this seed.
        let low_temperature_5a = high_power && target_c <= 60;
        Self {
            target_c,
            brake_distance_centi_c: if low_temperature_5a {
                1_800
            } else if target_c < 120 {
                450
            } else if target_c < 200 {
                700
            } else if high_temperature {
                // The 5 A HIL plant needs a high steady-state floor, but a
                // 1.5 C handoff lets residual heat drive a full-off/full-on
                // cycle. Begin the bounded approach at 2 C instead.
                200
            } else {
                1_000
            },
            warmup_power_permille: if low_temperature_5a { 750 } else { 1_000 },
            warmup_reenter_centi_c: if low_temperature_5a { 1_000 } else { 400 },
            approach_power_permille: if low_temperature_5a {
                450
            } else if high_temperature {
                600
            } else if high_power {
                360
            } else {
                320
            },
            approach_floor_power_permille: if low_temperature_5a {
                260
            } else if high_temperature {
                450
            } else if high_power {
                140
            } else {
                120
            },
            approach_damping_exponent_permille: if low_temperature_5a { 1_800 } else { 1_000 },
            approach_tail_window_centi_c: 0,
            hold_power_permille: if low_temperature_5a {
                60
            } else if high_temperature {
                600
            } else if high_power {
                190
            } else {
                160
            },
            hold_reheat_power_permille: if low_temperature_5a {
                60
            } else if high_temperature {
                600
            } else if high_power {
                80
            } else {
                60
            },
            hold_entry_centi_c: if low_temperature_5a { 200 } else { 90 },
            hold_exit_centi_c: if low_temperature_5a { 540 } else { 200 },
            hold_on_centi_c: 30,
            hold_off_centi_c: if low_temperature_5a { 120 } else { 5 },
            overshoot_cutoff_centi_c: if low_temperature_5a { 150 } else { 25 },
            hold_kp_permille_per_c: if low_temperature_5a {
                8
            } else if high_temperature {
                40
            } else if high_power {
                140
            } else {
                120
            },
            hold_ki_permille_per_c_tick: if low_temperature_5a {
                2
            } else if high_temperature {
                4
            } else if high_power {
                14
            } else {
                12
            },
            hold_blend_ticks: if low_temperature_5a { 1 } else { 12 },
            approach_lead_ticks: if low_temperature_5a { 4 } else { 0 },
            hold_lead_ticks: if low_temperature_5a { 2 } else { 0 },
        }
    }

    pub fn canonical_bytes(self, out: &mut [u8; CANDIDATE_POINT_CANONICAL_BYTES]) {
        let mut offset = 0;
        put_i16(out, &mut offset, self.target_c);
        for value in [
            self.brake_distance_centi_c,
            self.warmup_power_permille,
            self.warmup_reenter_centi_c,
            self.approach_power_permille,
            self.approach_floor_power_permille,
            self.approach_damping_exponent_permille,
            self.approach_tail_window_centi_c,
            self.hold_power_permille,
            self.hold_reheat_power_permille,
            self.hold_entry_centi_c,
            self.hold_exit_centi_c,
            self.hold_on_centi_c,
            self.hold_off_centi_c,
            self.overshoot_cutoff_centi_c,
            self.hold_kp_permille_per_c,
            self.hold_ki_permille_per_c_tick,
            self.hold_blend_ticks,
            self.approach_lead_ticks,
            self.hold_lead_ticks,
        ] {
            put_u16(out, &mut offset, value);
        }
    }

    pub fn from_canonical_bytes(bytes: &[u8; CANDIDATE_POINT_CANONICAL_BYTES]) -> Self {
        let mut offset = 0;
        let target_c = take_i16(bytes, &mut offset);
        let brake_distance_centi_c = take_u16(bytes, &mut offset);
        let warmup_power_permille = take_u16(bytes, &mut offset);
        let warmup_reenter_centi_c = take_u16(bytes, &mut offset);
        let approach_power_permille = take_u16(bytes, &mut offset);
        let approach_floor_power_permille = take_u16(bytes, &mut offset);
        let approach_damping_exponent_permille = take_u16(bytes, &mut offset);
        let approach_tail_window_centi_c = take_u16(bytes, &mut offset);
        let hold_power_permille = take_u16(bytes, &mut offset);
        let hold_reheat_power_permille = take_u16(bytes, &mut offset);
        let hold_entry_centi_c = take_u16(bytes, &mut offset);
        let hold_exit_centi_c = take_u16(bytes, &mut offset);
        let hold_on_centi_c = take_u16(bytes, &mut offset);
        let hold_off_centi_c = take_u16(bytes, &mut offset);
        let overshoot_cutoff_centi_c = take_u16(bytes, &mut offset);
        let hold_kp_permille_per_c = take_u16(bytes, &mut offset);
        let hold_ki_permille_per_c_tick = take_u16(bytes, &mut offset);
        let hold_blend_ticks = take_u16(bytes, &mut offset);
        let approach_lead_ticks = take_u16(bytes, &mut offset);
        let hold_lead_ticks = take_u16(bytes, &mut offset);

        Self {
            target_c,
            brake_distance_centi_c,
            warmup_power_permille,
            warmup_reenter_centi_c,
            approach_power_permille,
            approach_floor_power_permille,
            approach_damping_exponent_permille,
            approach_tail_window_centi_c,
            hold_power_permille,
            hold_reheat_power_permille,
            hold_entry_centi_c,
            hold_exit_centi_c,
            hold_on_centi_c,
            hold_off_centi_c,
            overshoot_cutoff_centi_c,
            hold_kp_permille_per_c,
            hold_ki_permille_per_c_tick,
            hold_blend_ticks,
            approach_lead_ticks,
            hold_lead_ticks,
        }
    }

    /// Interpolate a target-local seed from two accepted physical boundaries.
    /// All arithmetic is integer based so native, no_std and Wasm builds share
    /// the same rounding behavior.
    pub fn interpolate(lower: Self, upper: Self, target_c: i16) -> Self {
        if upper.target_c <= lower.target_c {
            return Self { target_c, ..lower };
        }
        let span = i32::from(upper.target_c) - i32::from(lower.target_c);
        let numerator = (i32::from(target_c) - i32::from(lower.target_c)).clamp(0, span);
        let lerp = |left: u16, right: u16| -> u16 {
            let delta = i64::from(right) - i64::from(left);
            let value = i64::from(left)
                + (delta * i64::from(numerator) + i64::from(span / 2)) / i64::from(span);
            value.clamp(0, i64::from(u16::MAX)) as u16
        };
        Self {
            target_c,
            brake_distance_centi_c: lerp(
                lower.brake_distance_centi_c,
                upper.brake_distance_centi_c,
            ),
            warmup_power_permille: lerp(lower.warmup_power_permille, upper.warmup_power_permille),
            warmup_reenter_centi_c: lerp(
                lower.warmup_reenter_centi_c,
                upper.warmup_reenter_centi_c,
            ),
            approach_power_permille: lerp(
                lower.approach_power_permille,
                upper.approach_power_permille,
            ),
            approach_floor_power_permille: lerp(
                lower.approach_floor_power_permille,
                upper.approach_floor_power_permille,
            ),
            approach_damping_exponent_permille: lerp(
                lower.approach_damping_exponent_permille,
                upper.approach_damping_exponent_permille,
            ),
            approach_tail_window_centi_c: lerp(
                lower.approach_tail_window_centi_c,
                upper.approach_tail_window_centi_c,
            ),
            hold_power_permille: lerp(lower.hold_power_permille, upper.hold_power_permille),
            hold_reheat_power_permille: lerp(
                lower.hold_reheat_power_permille,
                upper.hold_reheat_power_permille,
            ),
            hold_entry_centi_c: lerp(lower.hold_entry_centi_c, upper.hold_entry_centi_c),
            hold_exit_centi_c: lerp(lower.hold_exit_centi_c, upper.hold_exit_centi_c),
            hold_on_centi_c: lerp(lower.hold_on_centi_c, upper.hold_on_centi_c),
            hold_off_centi_c: lerp(lower.hold_off_centi_c, upper.hold_off_centi_c),
            overshoot_cutoff_centi_c: lerp(
                lower.overshoot_cutoff_centi_c,
                upper.overshoot_cutoff_centi_c,
            ),
            hold_kp_permille_per_c: lerp(
                lower.hold_kp_permille_per_c,
                upper.hold_kp_permille_per_c,
            ),
            hold_ki_permille_per_c_tick: lerp(
                lower.hold_ki_permille_per_c_tick,
                upper.hold_ki_permille_per_c_tick,
            ),
            hold_blend_ticks: lerp(lower.hold_blend_ticks, upper.hold_blend_ticks),
            approach_lead_ticks: lerp(lower.approach_lead_ticks, upper.approach_lead_ticks),
            hold_lead_ticks: lerp(lower.hold_lead_ticks, upper.hold_lead_ticks),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct CandidateScore {
    pub max_overshoot_centi: i32,
    pub hold_peak_to_peak_centi: i32,
    pub settle_ms: u32,
    pub hold_mean_absolute_error_centi: i32,
    pub output_switches: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CandidateGates {
    pub warmup_complete: bool,
    pub stage_complete: bool,
    pub overshoot: bool,
    pub hold_peak_to_peak: bool,
    pub hold_confirm: bool,
}

impl CandidateGates {
    pub const fn mask(self) -> u16 {
        gate_bit(self.warmup_complete, GATE_WARMUP_COMPLETE)
            | gate_bit(self.stage_complete, GATE_STAGE_COMPLETE)
            | gate_bit(self.overshoot, GATE_OVERSHOOT)
            | gate_bit(self.hold_peak_to_peak, GATE_HOLD_PEAK_TO_PEAK)
            | gate_bit(self.hold_confirm, GATE_HOLD_CONFIRM)
    }

    pub const fn passes(self) -> bool {
        self.warmup_complete
            && self.stage_complete
            && self.overshoot
            && self.hold_peak_to_peak
            && self.hold_confirm
    }
}

const fn gate_bit(enabled: bool, bit: u16) -> u16 {
    if enabled { bit } else { 0 }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CandidateEvaluation {
    pub point: CandidatePoint,
    pub score: CandidateScore,
    pub gates: CandidateGates,
}

/// Return the bounded, deterministic perturbation set for one target.
pub fn candidate_ladder(
    target_c: i16,
    power_class: PpsPowerClass,
) -> [CandidatePoint; CANDIDATE_LADDER_WIDTH] {
    candidate_ladder_from_seed_for_class(
        CandidatePoint::baseline(target_c, power_class),
        power_class,
    )
}

pub fn candidate_ladder_from_seed(
    seed: CandidatePoint,
) -> [CandidatePoint; CANDIDATE_LADDER_WIDTH] {
    candidate_ladder_from_seed_for_class(seed, PpsPowerClass::Pps3a)
}

fn candidate_ladder_from_seed_for_class(
    seed: CandidatePoint,
    power_class: PpsPowerClass,
) -> [CandidatePoint; CANDIDATE_LADDER_WIDTH] {
    let baseline = seed;
    let high_temperature_5a = matches!(power_class, PpsPowerClass::Pps5a) && seed.target_c >= 200;
    if high_temperature_5a {
        // At the high end of the 5 A plant, changing the brake distance at
        // the same time as the steady-state floor makes the three trials
        // incomparable. Keep the approach geometry fixed and bracket the
        // observed steady-state demand (600, 650, 700 permille).
        let mut sustained = baseline;
        sustained.approach_power_permille = sustained.approach_power_permille.saturating_add(50);
        sustained.approach_floor_power_permille =
            sustained.approach_floor_power_permille.saturating_add(50);
        sustained.hold_power_permille = sustained.hold_power_permille.saturating_add(50);
        sustained.hold_reheat_power_permille =
            sustained.hold_reheat_power_permille.saturating_add(50);

        let mut recovery = baseline;
        recovery.approach_power_permille = recovery.approach_power_permille.saturating_add(100);
        recovery.approach_floor_power_permille =
            recovery.approach_floor_power_permille.saturating_add(100);
        recovery.hold_power_permille = recovery.hold_power_permille.saturating_add(100);
        recovery.hold_reheat_power_permille =
            recovery.hold_reheat_power_permille.saturating_add(100);
        return [baseline, sustained, recovery];
    }
    if matches!(power_class, PpsPowerClass::Pps5a) && seed.target_c <= 60 {
        // Keep the low-temperature 5 A trials meaningfully distinct. A small
        // +/- 0.5 C brake change cannot identify a delayed, high-power plant.
        let mut conservative = baseline;
        conservative.brake_distance_centi_c = 2_150;
        conservative.warmup_power_permille = 650;
        conservative.approach_power_permille = 340;
        conservative.approach_floor_power_permille = 180;
        conservative.approach_damping_exponent_permille = 2_100;
        conservative.approach_lead_ticks = 10;

        let mut responsive = baseline;
        responsive.brake_distance_centi_c = 1_500;
        responsive.warmup_power_permille = 820;
        responsive.approach_power_permille = 520;
        responsive.approach_floor_power_permille = 340;
        responsive.approach_damping_exponent_permille = 1_500;
        responsive.approach_lead_ticks = 7;
        return [baseline, conservative, responsive];
    }
    let mut conservative = baseline;
    conservative.brake_distance_centi_c = baseline.brake_distance_centi_c.saturating_add(50);
    conservative.approach_power_permille = baseline.approach_power_permille.saturating_sub(20);
    conservative.approach_floor_power_permille =
        baseline.approach_floor_power_permille.saturating_sub(10);
    conservative.hold_power_permille = baseline.hold_power_permille.saturating_sub(10);
    conservative.hold_reheat_power_permille = conservative.hold_power_permille;

    let mut responsive = baseline;
    responsive.brake_distance_centi_c = baseline.brake_distance_centi_c.saturating_sub(50);
    responsive.approach_power_permille = baseline.approach_power_permille.saturating_add(20);
    responsive.approach_floor_power_permille =
        baseline.approach_floor_power_permille.saturating_add(10);
    responsive.hold_power_permille = baseline.hold_power_permille.saturating_add(10).min(1_000);
    responsive.hold_reheat_power_permille = responsive.hold_power_permille;
    [baseline, conservative, responsive]
}

pub fn select_candidate(
    target_c: i16,
    power_class: PpsPowerClass,
    evaluations: &[CandidateEvaluation; CANDIDATE_LADDER_WIDTH],
) -> Option<CandidatePoint> {
    let ladder = candidate_ladder(target_c, power_class);
    select_candidate_from_ladder(&ladder, evaluations)
}

pub fn select_candidate_from_ladder(
    ladder: &[CandidatePoint; CANDIDATE_LADDER_WIDTH],
    evaluations: &[CandidateEvaluation; CANDIDATE_LADDER_WIDTH],
) -> Option<CandidatePoint> {
    let mut selected: Option<CandidateEvaluation> = None;
    for evaluation in evaluations {
        if !evaluation.gates.passes() || !ladder.contains(&evaluation.point) {
            continue;
        }
        let replace = selected.is_none_or(|current| {
            evaluation.score < current.score
                || (evaluation.score == current.score
                    && canonical_point_bytes(evaluation.point)
                        < canonical_point_bytes(current.point))
        });
        if replace {
            selected = Some(*evaluation);
        }
    }
    selected.map(|evaluation| evaluation.point)
}

fn canonical_point_bytes(point: CandidatePoint) -> [u8; CANDIDATE_POINT_CANONICAL_BYTES] {
    let mut bytes = [0u8; CANDIDATE_POINT_CANONICAL_BYTES];
    point.canonical_bytes(&mut bytes);
    bytes
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CandidateProfile {
    pub power_class: PpsPowerClass,
    pub points: [CandidatePoint; TARGET_COUNT],
}

impl CandidateProfile {
    pub const fn baseline(power_class: PpsPowerClass) -> Self {
        Self {
            power_class,
            points: [
                CandidatePoint::baseline(60, power_class),
                CandidatePoint::baseline(80, power_class),
                CandidatePoint::baseline(100, power_class),
                CandidatePoint::baseline(120, power_class),
                CandidatePoint::baseline(140, power_class),
                CandidatePoint::baseline(160, power_class),
                CandidatePoint::baseline(180, power_class),
                CandidatePoint::baseline(220, power_class),
                CandidatePoint::baseline(240, power_class),
            ],
        }
    }

    pub fn canonical_bytes(self, out: &mut [u8; CANDIDATE_PROFILE_CANONICAL_BYTES]) {
        out[0] = match self.power_class {
            PpsPowerClass::Pps3a => 3,
            PpsPowerClass::Pps5a => 5,
        };
        for (index, point) in self.points.into_iter().enumerate() {
            let start = 1 + index * CANDIDATE_POINT_CANONICAL_BYTES;
            let mut point_bytes = [0u8; CANDIDATE_POINT_CANONICAL_BYTES];
            point.canonical_bytes(&mut point_bytes);
            out[start..start + CANDIDATE_POINT_CANONICAL_BYTES].copy_from_slice(&point_bytes);
        }
    }

    pub fn from_canonical_bytes(bytes: &[u8; CANDIDATE_PROFILE_CANONICAL_BYTES]) -> Option<Self> {
        let power_class = match bytes[0] {
            3 => PpsPowerClass::Pps3a,
            5 => PpsPowerClass::Pps5a,
            _ => return None,
        };
        let mut profile = Self::baseline(power_class);
        for (index, target_c) in PHYSICAL_TARGETS_C.iter().copied().enumerate() {
            let start = 1 + index * CANDIDATE_POINT_CANONICAL_BYTES;
            let mut point_bytes = [0u8; CANDIDATE_POINT_CANONICAL_BYTES];
            point_bytes.copy_from_slice(&bytes[start..start + CANDIDATE_POINT_CANONICAL_BYTES]);
            let point = CandidatePoint::from_canonical_bytes(&point_bytes);
            if point.target_c != target_c {
                return None;
            }
            profile.points[index] = point;
        }
        Some(profile)
    }

    pub fn hash(self) -> [u8; 32] {
        let mut bytes = [0u8; CANDIDATE_PROFILE_CANONICAL_BYTES];
        self.canonical_bytes(&mut bytes);
        sha256(&bytes)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CandidateIdentity {
    pub candidate_id: [u8; 16],
    pub candidate_hash: [u8; 32],
}

impl CandidateIdentity {
    pub fn from_profile(profile: CandidateProfile) -> Self {
        let hash = profile.hash();
        let mut candidate_id = [0u8; 16];
        candidate_id.copy_from_slice(&hash[..16]);
        Self {
            candidate_id,
            candidate_hash: hash,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SampleEvent {
    pub elapsed_ms: u32,
    pub target_c: i16,
    pub trial_index: u8,
    pub candidate_hash: [u8; 32],
    pub temperature_centi_c: i16,
    pub vin_mv: u16,
    pub pps_contract_mv: u16,
    pub pps_contract_ma: u16,
    pub heater_output_permille: u16,
    pub measurement_valid: bool,
    pub phase: Phase,
    pub heater_phase: HeaterPhase,
}

/// The actual heater controller phase, recorded independently from the
/// tuning workflow phase so approach timing remains auditable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaterPhase {
    Warmup,
    Approach,
    Hold,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecisionEvent {
    pub elapsed_ms: u32,
    pub target_c: i16,
    pub disposition: TargetDisposition,
    pub score_tracking: i32,
    pub score_energy: i32,
    pub score_overshoot: i32,
    pub score_stability: i32,
    pub score_settle_ms: u32,
    pub score_hold_mean_absolute_error_centi: i32,
    pub score_output_switches: u16,
    pub interval_lower_boundary_c: i16,
    pub interval_upper_boundary_c: i16,
    pub interval_pruned: bool,
    pub candidate_frozen: bool,
    pub gates: u16,
    pub candidate_hash: [u8; 32],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CandidateTrialBoundary {
    Started,
    Completed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CandidateTrialEvent {
    pub elapsed_ms: u32,
    pub target_c: i16,
    pub trial_index: u8,
    pub boundary: CandidateTrialBoundary,
    pub point: CandidatePoint,
    pub candidate_hash: [u8; 32],
    pub start_sequence: u64,
    pub start_elapsed_ms: u32,
    pub score: CandidateScore,
    pub gates: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhaseTransitionEvent {
    pub elapsed_ms: u32,
    pub target_c: i16,
    pub trial_index: u8,
    pub previous_phase: Phase,
    pub phase: Phase,
    pub reason: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SafetyEvent {
    pub elapsed_ms: u32,
    pub target_c: i16,
    pub trial_index: u8,
    pub reason: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraceEvent {
    Sample(SampleEvent),
    PhaseTransition(PhaseTransitionEvent),
    CandidateTrial(CandidateTrialEvent),
    Decision(DecisionEvent),
    Safety(SafetyEvent),
}

impl TraceEvent {
    fn canonical_bytes(self, sequence: u64, out: &mut [u8; 128]) -> usize {
        let mut offset = 0;
        put_u64(out, &mut offset, sequence);
        match self {
            Self::Sample(sample) => {
                out[offset] = 1;
                offset += 1;
                put_u32(out, &mut offset, sample.elapsed_ms);
                put_i16(out, &mut offset, sample.target_c);
                out[offset] = sample.trial_index;
                offset += 1;
                out[offset..offset + 32].copy_from_slice(&sample.candidate_hash);
                offset += 32;
                put_i16(out, &mut offset, sample.temperature_centi_c);
                put_u16(out, &mut offset, sample.vin_mv);
                put_u16(out, &mut offset, sample.pps_contract_mv);
                put_u16(out, &mut offset, sample.pps_contract_ma);
                put_u16(out, &mut offset, sample.heater_output_permille);
                out[offset] = u8::from(sample.measurement_valid);
                offset += 1;
                out[offset] = phase_byte(sample.phase);
                offset += 1;
                out[offset] = heater_phase_byte(sample.heater_phase);
                offset + 1
            }
            Self::Decision(decision) => {
                out[offset] = 2;
                offset += 1;
                put_u32(out, &mut offset, decision.elapsed_ms);
                put_i16(out, &mut offset, decision.target_c);
                out[offset] = decision.disposition.as_byte();
                offset += 1;
                put_i32(out, &mut offset, decision.score_tracking);
                put_i32(out, &mut offset, decision.score_energy);
                put_i32(out, &mut offset, decision.score_overshoot);
                put_i32(out, &mut offset, decision.score_stability);
                put_u32(out, &mut offset, decision.score_settle_ms);
                put_i32(
                    out,
                    &mut offset,
                    decision.score_hold_mean_absolute_error_centi,
                );
                put_u16(out, &mut offset, decision.score_output_switches);
                put_i16(out, &mut offset, decision.interval_lower_boundary_c);
                put_i16(out, &mut offset, decision.interval_upper_boundary_c);
                out[offset] = u8::from(decision.interval_pruned);
                offset += 1;
                out[offset] = u8::from(decision.candidate_frozen);
                offset += 1;
                put_u16(out, &mut offset, decision.gates);
                out[offset..offset + 32].copy_from_slice(&decision.candidate_hash);
                offset + 32
            }
            Self::PhaseTransition(transition) => {
                out[offset] = 3;
                offset += 1;
                put_u32(out, &mut offset, transition.elapsed_ms);
                put_i16(out, &mut offset, transition.target_c);
                out[offset] = transition.trial_index;
                offset += 1;
                out[offset] = phase_byte(transition.previous_phase);
                offset += 1;
                out[offset] = phase_byte(transition.phase);
                offset += 1;
                put_u16(out, &mut offset, transition.reason);
                offset
            }
            Self::CandidateTrial(trial) => {
                out[offset] = 4;
                offset += 1;
                put_u32(out, &mut offset, trial.elapsed_ms);
                put_i16(out, &mut offset, trial.target_c);
                out[offset] = trial.trial_index;
                offset += 1;
                out[offset] = match trial.boundary {
                    CandidateTrialBoundary::Started => 1,
                    CandidateTrialBoundary::Completed => 2,
                };
                offset += 1;
                let mut point = [0; CANDIDATE_POINT_CANONICAL_BYTES];
                trial.point.canonical_bytes(&mut point);
                out[offset..offset + CANDIDATE_POINT_CANONICAL_BYTES].copy_from_slice(&point);
                offset += CANDIDATE_POINT_CANONICAL_BYTES;
                out[offset..offset + 32].copy_from_slice(&trial.candidate_hash);
                offset += 32;
                put_u64(out, &mut offset, trial.start_sequence);
                put_u32(out, &mut offset, trial.start_elapsed_ms);
                put_i32(out, &mut offset, trial.score.max_overshoot_centi);
                put_i32(out, &mut offset, trial.score.hold_peak_to_peak_centi);
                put_u32(out, &mut offset, trial.score.settle_ms);
                put_i32(out, &mut offset, trial.score.hold_mean_absolute_error_centi);
                put_u16(out, &mut offset, trial.score.output_switches);
                put_u16(out, &mut offset, trial.gates);
                offset
            }
            Self::Safety(safety) => {
                out[offset] = 5;
                offset += 1;
                put_u32(out, &mut offset, safety.elapsed_ms);
                put_i16(out, &mut offset, safety.target_c);
                out[offset] = safety.trial_index;
                offset += 1;
                put_u16(out, &mut offset, safety.reason);
                offset
            }
        }
    }

    pub const fn is_sample(self) -> bool {
        matches!(self, Self::Sample(_))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TraceRecord {
    pub sequence: u64,
    pub event: TraceEvent,
    pub digest: [u8; 32],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct IntervalDecision {
    pub lower_boundary_c: i16,
    pub upper_boundary_c: i16,
    pub pruned: bool,
    pub candidate_frozen: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunSummary {
    pub run_id: u64,
    pub state: RunState,
    pub power_class: Option<PpsPowerClass>,
    pub phase: Phase,
    pub current_target_c: Option<i16>,
    pub accepted: [bool; TARGET_COUNT],
    pub failed: [bool; TARGET_COUNT],
    pub skipped: [bool; TARGET_COUNT],
    pub terminal: Option<TerminalDisposition>,
    pub review_complete: bool,
    pub trace_gap: bool,
    pub first_sequence: Option<u64>,
    pub last_sequence: Option<u64>,
    pub acknowledged_through: Option<u64>,
    pub trace_digest: [u8; 32],
    pub candidate: Option<CandidateIdentity>,
    pub promotion: PromotionState,
}

pub struct ThermalTuningCore<const TRACE_CAP: usize = TRACE_EVENT_CAPACITY> {
    run_id: u64,
    state: RunState,
    power_class: Option<PpsPowerClass>,
    phase: Phase,
    current_target_index: usize,
    dispositions: [TargetDisposition; TARGET_COUNT],
    terminal: Option<TerminalDisposition>,
    candidate: Option<CandidateProfile>,
    candidate_identity: Option<CandidateIdentity>,
    promotion: PromotionState,
    trace: [Option<TraceRecord>; TRACE_CAP],
    first_sequence: u64,
    next_sequence: u64,
    acknowledged_through: Option<u64>,
    acknowledged_digest: Option<[u8; 32]>,
    trace_digest: [u8; 32],
    trace_gap: bool,
    candidate_frozen_current: bool,
}

impl<const TRACE_CAP: usize> ThermalTuningCore<TRACE_CAP> {
    pub fn new() -> Self {
        Self {
            run_id: 0,
            state: RunState::Idle,
            power_class: None,
            phase: Phase::Idle,
            current_target_index: 0,
            dispositions: [TargetDisposition::Pending; TARGET_COUNT],
            terminal: None,
            candidate: None,
            candidate_identity: None,
            promotion: PromotionState::Unavailable,
            trace: core::array::from_fn(|_| None),
            first_sequence: 0,
            next_sequence: 0,
            acknowledged_through: None,
            acknowledged_digest: None,
            trace_digest: [0; 32],
            trace_gap: false,
            candidate_frozen_current: false,
        }
    }

    /// Initializes a core directly at `out` without materializing the trace
    /// ring as a temporary value on the caller's stack.
    ///
    /// # Safety
    ///
    /// `out` must be valid, properly aligned, writable storage for one
    /// uninitialized `Self`. The storage must not already contain a live value.
    pub unsafe fn init_in_place(out: *mut Self) {
        unsafe {
            core::ptr::addr_of_mut!((*out).run_id).write(0);
            core::ptr::addr_of_mut!((*out).state).write(RunState::Idle);
            core::ptr::addr_of_mut!((*out).power_class).write(None);
            core::ptr::addr_of_mut!((*out).phase).write(Phase::Idle);
            core::ptr::addr_of_mut!((*out).current_target_index).write(0);
            core::ptr::addr_of_mut!((*out).dispositions)
                .write([TargetDisposition::Pending; TARGET_COUNT]);
            core::ptr::addr_of_mut!((*out).terminal).write(None);
            core::ptr::addr_of_mut!((*out).candidate).write(None);
            core::ptr::addr_of_mut!((*out).candidate_identity).write(None);
            core::ptr::addr_of_mut!((*out).promotion).write(PromotionState::Unavailable);
            let trace = core::ptr::addr_of_mut!((*out).trace).cast::<Option<TraceRecord>>();
            for index in 0..TRACE_CAP {
                trace.add(index).write(None);
            }
            core::ptr::addr_of_mut!((*out).first_sequence).write(0);
            core::ptr::addr_of_mut!((*out).next_sequence).write(0);
            core::ptr::addr_of_mut!((*out).acknowledged_through).write(None);
            core::ptr::addr_of_mut!((*out).acknowledged_digest).write(None);
            core::ptr::addr_of_mut!((*out).trace_digest).write([0; 32]);
            core::ptr::addr_of_mut!((*out).trace_gap).write(false);
            core::ptr::addr_of_mut!((*out).candidate_frozen_current).write(false);
        }
    }

    pub fn start(
        &mut self,
        run_id: u64,
        power_class: PpsPowerClass,
        eligibility: Eligibility,
    ) -> Result<(), StartError> {
        if TRACE_CAP == 0 {
            return Err(StartError::InvalidTraceCapacity);
        }
        if !eligibility.ready() {
            return Err(StartError::Ineligible);
        }
        if self.state == RunState::Running {
            return Err(StartError::Busy);
        }
        self.run_id = run_id;
        self.state = RunState::Running;
        self.power_class = Some(power_class);
        self.phase = Phase::CooldownWait;
        self.current_target_index = 0;
        self.dispositions = [TargetDisposition::Pending; TARGET_COUNT];
        self.terminal = None;
        let profile = CandidateProfile::baseline(power_class);
        self.candidate_identity = Some(CandidateIdentity::from_profile(profile));
        self.candidate = Some(profile);
        self.promotion = PromotionState::AwaitingReview;
        self.clear_trace();
        self.first_sequence = 0;
        self.next_sequence = 0;
        self.acknowledged_through = None;
        self.acknowledged_digest = None;
        self.trace_digest = [0; 32];
        self.trace_gap = false;
        self.candidate_frozen_current = false;
        Ok(())
    }

    pub const fn run_id(&self) -> u64 {
        self.run_id
    }
    pub const fn state(&self) -> RunState {
        self.state
    }
    pub const fn phase(&self) -> Phase {
        self.phase
    }
    pub const fn power_class(&self) -> Option<PpsPowerClass> {
        self.power_class
    }
    pub const fn candidate(&self) -> Option<CandidateProfile> {
        self.candidate
    }
    pub const fn candidate_identity(&self) -> Option<CandidateIdentity> {
        self.candidate_identity
    }
    pub const fn trace_gap(&self) -> bool {
        self.trace_gap
    }
    pub const fn trace_digest(&self) -> [u8; 32] {
        self.trace_digest
    }
    pub const fn terminal(&self) -> Option<TerminalDisposition> {
        self.terminal
    }
    pub const fn promotion(&self) -> PromotionState {
        self.promotion
    }
    pub fn current_target(&self) -> Option<i16> {
        if self.state != RunState::Running || self.current_target_index >= TARGET_COUNT {
            None
        } else {
            Some(EXECUTION_ORDER_C[self.current_target_index])
        }
    }

    /// Generate the current target's bounded ladder from the nearest accepted
    /// physical boundaries. If both boundaries are unavailable, use the class
    /// baseline as the deterministic seed.
    pub fn candidate_ladder_for_current_target(
        &self,
    ) -> Option<[CandidatePoint; CANDIDATE_LADDER_WIDTH]> {
        let target_c = self.current_target()?;
        let power_class = self.power_class?;
        let profile = self.candidate?;
        let mut lower: Option<CandidatePoint> = None;
        let mut upper: Option<CandidatePoint> = None;
        for point in profile.points {
            let Some(execution_index) = EXECUTION_ORDER_C
                .iter()
                .position(|candidate_target| *candidate_target == point.target_c)
            else {
                continue;
            };
            if self.dispositions[execution_index] != TargetDisposition::Accepted {
                continue;
            }
            if point.target_c < target_c
                && lower.is_none_or(|candidate| candidate.target_c < point.target_c)
            {
                lower = Some(point);
            }
            if point.target_c > target_c
                && upper.is_none_or(|candidate| candidate.target_c > point.target_c)
            {
                upper = Some(point);
            }
        }
        let seed = match (lower, upper) {
            (Some(lower), Some(upper)) => CandidatePoint::interpolate(lower, upper, target_c),
            _ => CandidatePoint::baseline(target_c, power_class),
        };
        Some(candidate_ladder_from_seed_for_class(seed, power_class))
    }

    pub fn set_phase(&mut self, phase: Phase) -> Result<(), TraceError> {
        if self.state != RunState::Running {
            return Err(TraceError::NotRunning);
        }
        self.phase = phase;
        Ok(())
    }

    pub fn record_sample(&mut self, sample: SampleEvent) -> Result<u64, TraceError> {
        self.record(TraceEvent::Sample(sample))
    }

    pub fn record_decision(&mut self, decision: DecisionEvent) -> Result<u64, TraceError> {
        self.record(TraceEvent::Decision(decision))
    }

    pub fn record_phase_transition(
        &mut self,
        transition: PhaseTransitionEvent,
    ) -> Result<u64, TraceError> {
        self.phase = transition.phase;
        self.record(TraceEvent::PhaseTransition(transition))
    }

    pub fn record_candidate_trial(
        &mut self,
        trial: CandidateTrialEvent,
    ) -> Result<u64, TraceError> {
        self.record(TraceEvent::CandidateTrial(trial))
    }

    pub fn record_safety(&mut self, safety: SafetyEvent) -> Result<u64, TraceError> {
        self.record(TraceEvent::Safety(safety))
    }

    pub const fn next_sequence(&self) -> u64 {
        self.next_sequence
    }

    fn clear_trace(&mut self) {
        // `start` runs against firmware-owned PSRAM. Assigning a freshly
        // constructed `[Option<TraceRecord>; TRACE_CAP]` here would first put
        // the whole ring on the control task stack. Clear each external slot
        // in place instead.
        for slot in &mut self.trace {
            *slot = None;
        }
    }

    fn record(&mut self, event: TraceEvent) -> Result<u64, TraceError> {
        if self.state != RunState::Running || TRACE_CAP == 0 {
            return Err(TraceError::NotRunning);
        }
        let sequence = self.next_sequence;
        let mut canonical = [0u8; 128];
        let length = event.canonical_bytes(sequence, &mut canonical);
        let mut digest_input = [0u8; 160];
        digest_input[..32].copy_from_slice(&self.trace_digest);
        digest_input[32..32 + length].copy_from_slice(&canonical[..length]);
        let digest = sha256(&digest_input[..32 + length]);
        let slot = (sequence as usize) % TRACE_CAP;
        if let Some(previous) = self.trace[slot]
            && previous.sequence >= self.first_sequence
            && self
                .acknowledged_through
                .is_none_or(|ack| previous.sequence > ack)
        {
            self.trace_gap = true;
        }
        self.trace[slot] = Some(TraceRecord {
            sequence,
            event,
            digest,
        });
        if self.next_sequence.saturating_sub(self.first_sequence) >= TRACE_CAP as u64 {
            self.first_sequence = self.first_sequence.saturating_add(1);
        }
        self.next_sequence = self.next_sequence.saturating_add(1);
        self.trace_digest = digest;
        Ok(sequence)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn complete_current_target(
        &mut self,
        disposition: TargetDisposition,
        elapsed_ms: u32,
        score_tracking: i32,
        score_energy: i32,
        score_overshoot: i32,
        score_stability: i32,
        score_settle_ms: u32,
        score_hold_mean_absolute_error_centi: i32,
        score_output_switches: u16,
        gates: u16,
    ) -> Result<(), TraceError> {
        if self.state != RunState::Running {
            return Err(TraceError::NotRunning);
        }
        let target_c = EXECUTION_ORDER_C[self.current_target_index];
        let candidate_hash = self
            .candidate_identity
            .map_or([0; 32], |identity| identity.candidate_hash);
        self.dispositions[self.current_target_index] = disposition;
        let interval = self.interval_for_current_target(target_c, disposition);
        self.record_decision(DecisionEvent {
            elapsed_ms,
            target_c,
            disposition,
            score_tracking,
            score_energy,
            score_overshoot,
            score_stability,
            score_settle_ms,
            score_hold_mean_absolute_error_centi,
            score_output_switches,
            interval_lower_boundary_c: interval.lower_boundary_c,
            interval_upper_boundary_c: interval.upper_boundary_c,
            interval_pruned: interval.pruned,
            candidate_frozen: interval.candidate_frozen,
            gates,
            candidate_hash,
        })?;
        self.candidate_frozen_current = false;
        self.current_target_index = self.current_target_index.saturating_add(1);
        if self.current_target_index >= TARGET_COUNT {
            let terminal = if self
                .dispositions
                .iter()
                .all(|disposition| *disposition == TargetDisposition::Accepted)
            {
                TerminalDisposition::Completed
            } else {
                TerminalDisposition::Failed
            };
            self.finish(terminal)?;
        } else {
            self.phase = Phase::CooldownWait;
        }
        Ok(())
    }

    /// Freeze the winning ladder point into the current profile before the
    /// target decision is emitted. A failed ladder leaves the profile intact.
    pub fn freeze_current_target(
        &mut self,
        evaluations: &[CandidateEvaluation; CANDIDATE_LADDER_WIDTH],
    ) -> Result<Option<CandidateIdentity>, TraceError> {
        if self.state != RunState::Running {
            return Err(TraceError::NotRunning);
        }
        let target_c = EXECUTION_ORDER_C[self.current_target_index];
        let Some(ladder) = self.candidate_ladder_for_current_target() else {
            return Ok(None);
        };
        let Some(point) = select_candidate_from_ladder(&ladder, evaluations) else {
            return Ok(None);
        };
        let Some(mut profile) = self.candidate else {
            return Ok(None);
        };
        if let Some(index) = profile
            .points
            .iter()
            .position(|candidate| candidate.target_c == target_c)
        {
            profile.points[index] = point;
            let identity = CandidateIdentity::from_profile(profile);
            self.candidate = Some(profile);
            self.candidate_identity = Some(identity);
            self.candidate_frozen_current = true;
            return Ok(Some(identity));
        }
        Ok(None)
    }

    /// Apply one ladder point to the current target while the firmware runs
    /// that candidate's observation window. This deliberately updates the
    /// in-RAM identity but does not mark the point accepted; acceptance still
    /// goes through `freeze_current_target` after all candidate windows close.
    pub fn set_current_target_candidate(
        &mut self,
        point: CandidatePoint,
    ) -> Result<Option<CandidateIdentity>, TraceError> {
        if self.state != RunState::Running {
            return Err(TraceError::NotRunning);
        }
        let Some(target_c) = self.current_target() else {
            return Ok(None);
        };
        if point.target_c != target_c {
            return Ok(None);
        }
        let Some(mut profile) = self.candidate else {
            return Ok(None);
        };
        let Some(index) = profile
            .points
            .iter()
            .position(|candidate| candidate.target_c == target_c)
        else {
            return Ok(None);
        };
        profile.points[index] = point;
        let identity = CandidateIdentity::from_profile(profile);
        self.candidate = Some(profile);
        self.candidate_identity = Some(identity);
        self.candidate_frozen_current = false;
        Ok(Some(identity))
    }

    /// Records a terminal/diagnostic decision without advancing the scheduler.
    #[allow(clippy::too_many_arguments)]
    pub fn record_current_decision(
        &mut self,
        disposition: TargetDisposition,
        elapsed_ms: u32,
        score_tracking: i32,
        score_energy: i32,
        score_overshoot: i32,
        score_stability: i32,
        score_settle_ms: u32,
        score_hold_mean_absolute_error_centi: i32,
        score_output_switches: u16,
        gates: u16,
    ) -> Result<(), TraceError> {
        if self.state != RunState::Running {
            return Err(TraceError::NotRunning);
        }
        let index = self.current_target_index.min(TARGET_COUNT - 1);
        let target_c = EXECUTION_ORDER_C[index];
        let candidate_hash = self
            .candidate_identity
            .map_or([0; 32], |identity| identity.candidate_hash);
        self.dispositions[index] = disposition;
        let interval = self.interval_for_current_target(target_c, disposition);
        self.record_decision(DecisionEvent {
            elapsed_ms,
            target_c,
            disposition,
            score_tracking,
            score_energy,
            score_overshoot,
            score_stability,
            score_settle_ms,
            score_hold_mean_absolute_error_centi,
            score_output_switches,
            interval_lower_boundary_c: interval.lower_boundary_c,
            interval_upper_boundary_c: interval.upper_boundary_c,
            interval_pruned: interval.pruned,
            candidate_frozen: interval.candidate_frozen,
            gates,
            candidate_hash,
        })?;
        self.candidate_frozen_current = false;
        Ok(())
    }

    fn interval_for_current_target(
        &self,
        target_c: i16,
        disposition: TargetDisposition,
    ) -> IntervalDecision {
        let mut lower_boundary_c = PHYSICAL_TARGETS_C[0];
        let mut upper_boundary_c = PHYSICAL_TARGETS_C[TARGET_COUNT - 1];
        for (index, accepted_target_c) in EXECUTION_ORDER_C.into_iter().enumerate() {
            if accepted_target_c == target_c
                || self.dispositions[index] != TargetDisposition::Accepted
            {
                continue;
            }
            if accepted_target_c < target_c && accepted_target_c > lower_boundary_c {
                lower_boundary_c = accepted_target_c;
            }
            if accepted_target_c > target_c && accepted_target_c < upper_boundary_c {
                upper_boundary_c = accepted_target_c;
            }
        }
        IntervalDecision {
            lower_boundary_c,
            upper_boundary_c,
            pruned: matches!(
                disposition,
                TargetDisposition::Failed | TargetDisposition::Skipped
            ),
            candidate_frozen: self.candidate_frozen_current,
        }
    }

    pub fn finish(&mut self, disposition: TerminalDisposition) -> Result<(), TraceError> {
        if self.state != RunState::Running {
            return Err(TraceError::NotRunning);
        }
        self.state = RunState::Terminal;
        self.phase = Phase::Terminal;
        self.terminal = Some(disposition);
        self.promotion = if disposition.is_success() {
            PromotionState::AwaitingReview
        } else {
            PromotionState::Unavailable
        };
        Ok(())
    }

    pub fn ack_trace(&mut self, through_sequence: u64, digest: [u8; 32]) -> Result<(), TraceError> {
        if self.trace_gap {
            return Err(TraceError::Gap);
        }
        if self.acknowledged_through == Some(through_sequence) {
            return if self.acknowledged_digest == Some(digest) {
                Ok(())
            } else {
                Err(TraceError::DigestMismatch)
            };
        }
        let first = self.first_sequence;
        let last = self.next_sequence.checked_sub(1).ok_or(TraceError::Range)?;
        let expected = self
            .acknowledged_through
            .map_or(first, |value| value.saturating_add(1));
        if through_sequence < expected || through_sequence > last || expected < first {
            return Err(TraceError::Range);
        }
        if self.digest_at(through_sequence) != Some(digest) {
            return Err(TraceError::DigestMismatch);
        }
        self.acknowledged_through = Some(through_sequence);
        self.acknowledged_digest = Some(digest);
        Ok(())
    }

    pub fn seal_review(
        &mut self,
        through_sequence: u64,
        digest: [u8; 32],
    ) -> Result<(), TraceError> {
        if self.state != RunState::Terminal {
            return Err(TraceError::NotTerminal);
        }
        if self.trace_gap {
            self.promotion = PromotionState::Unavailable;
            return Err(TraceError::Gap);
        }
        let terminal_sequence = self.next_sequence.checked_sub(1).ok_or(TraceError::Range)?;
        if self.acknowledged_through == Some(through_sequence) {
            if self.acknowledged_digest != Some(digest) {
                self.promotion = PromotionState::Unavailable;
                return Err(TraceError::DigestMismatch);
            }
        } else {
            self.ack_trace(through_sequence, digest)?;
        }
        if through_sequence != terminal_sequence || digest != self.trace_digest {
            self.promotion = PromotionState::Unavailable;
            return Err(TraceError::DigestMismatch);
        }
        if self.terminal.is_some_and(TerminalDisposition::is_success) {
            self.promotion = PromotionState::Ready;
            Ok(())
        } else {
            self.promotion = PromotionState::Unavailable;
            Err(TraceError::NotTerminal)
        }
    }

    pub fn preview(
        &mut self,
        run_id: u64,
        identity: CandidateIdentity,
        power_class: PpsPowerClass,
    ) -> Result<CandidateProfile, PromotionError> {
        self.validate_candidate(run_id, identity, power_class)?;
        if self.promotion != PromotionState::Ready {
            return Err(PromotionError::ReviewIncomplete);
        }
        self.promotion = PromotionState::Previewed;
        self.candidate.ok_or(PromotionError::Unavailable)
    }

    pub fn discard_preview(
        &mut self,
        run_id: u64,
        identity: CandidateIdentity,
        power_class: PpsPowerClass,
    ) -> Result<(), PromotionError> {
        self.validate_candidate(run_id, identity, power_class)?;
        if self.promotion != PromotionState::Previewed {
            return Err(PromotionError::Unavailable);
        }
        self.promotion = PromotionState::Ready;
        Ok(())
    }

    pub fn save(
        &mut self,
        run_id: u64,
        identity: CandidateIdentity,
        power_class: PpsPowerClass,
    ) -> Result<CandidateProfile, PromotionError> {
        self.validate_candidate(run_id, identity, power_class)?;
        if self.promotion != PromotionState::Previewed {
            return Err(PromotionError::NotPreviewed);
        }
        self.promotion = PromotionState::Saved;
        self.candidate.ok_or(PromotionError::Unavailable)
    }

    fn validate_candidate(
        &self,
        run_id: u64,
        identity: CandidateIdentity,
        power_class: PpsPowerClass,
    ) -> Result<(), PromotionError> {
        if self.run_id != run_id
            || self.power_class != Some(power_class)
            || self.candidate_identity != Some(identity)
        {
            return Err(PromotionError::Mismatch);
        }
        Ok(())
    }

    pub fn trace_page(
        &self,
        after_sequence: Option<u64>,
        limit: usize,
        out: &mut [TraceRecord],
    ) -> usize {
        let start = after_sequence.map_or(self.first_sequence, |value| value.saturating_add(1));
        let end = self.next_sequence;
        let mut written = 0;
        let max = limit.min(out.len());
        let mut sequence = start;
        while sequence < end && written < max {
            if let Some(record) = self.record_at(sequence) {
                out[written] = record;
                written += 1;
            }
            sequence = sequence.saturating_add(1);
        }
        written
    }

    fn record_at(&self, sequence: u64) -> Option<TraceRecord> {
        if sequence < self.first_sequence || sequence >= self.next_sequence || TRACE_CAP == 0 {
            return None;
        }
        self.trace[(sequence as usize) % TRACE_CAP].filter(|record| record.sequence == sequence)
    }

    pub fn trace_record(&self, sequence: u64) -> Option<TraceRecord> {
        self.record_at(sequence)
    }

    fn digest_at(&self, sequence: u64) -> Option<[u8; 32]> {
        self.record_at(sequence).map(|record| record.digest)
    }

    pub fn summary(&self) -> RunSummary {
        let mut accepted = [false; TARGET_COUNT];
        let mut failed = [false; TARGET_COUNT];
        let mut skipped = [false; TARGET_COUNT];
        for (index, disposition) in self.dispositions.into_iter().enumerate() {
            match disposition {
                TargetDisposition::Accepted => accepted[index] = true,
                TargetDisposition::Failed => failed[index] = true,
                TargetDisposition::Skipped => skipped[index] = true,
                TargetDisposition::Pending => {}
            }
        }
        RunSummary {
            run_id: self.run_id,
            state: self.state,
            power_class: self.power_class,
            phase: self.phase,
            current_target_c: self.current_target(),
            accepted,
            failed,
            skipped,
            terminal: self.terminal,
            review_complete: self.promotion != PromotionState::Unavailable
                && self.promotion != PromotionState::AwaitingReview,
            trace_gap: self.trace_gap,
            first_sequence: (self.next_sequence > self.first_sequence)
                .then_some(self.first_sequence),
            last_sequence: self.next_sequence.checked_sub(1),
            acknowledged_through: self.acknowledged_through,
            trace_digest: self.trace_digest,
            candidate: self.candidate_identity,
            promotion: self.promotion,
        }
    }
}

impl<const TRACE_CAP: usize> Default for ThermalTuningCore<TRACE_CAP> {
    fn default() -> Self {
        Self::new()
    }
}

pub fn sha256(bytes: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut output = [0u8; 32];
    output.copy_from_slice(&digest);
    output
}

fn phase_byte(value: Phase) -> u8 {
    match value {
        Phase::Idle => 0,
        Phase::CooldownWait => 1,
        Phase::Scout => 2,
        Phase::Retune => 3,
        Phase::HoldConfirm => 4,
        Phase::Terminal => 5,
    }
}

const fn heater_phase_byte(value: HeaterPhase) -> u8 {
    match value {
        HeaterPhase::Warmup => 1,
        HeaterPhase::Approach => 2,
        HeaterPhase::Hold => 3,
    }
}

fn put_u16(out: &mut [u8], offset: &mut usize, value: u16) {
    out[*offset..*offset + 2].copy_from_slice(&value.to_le_bytes());
    *offset += 2;
}

fn take_u16(bytes: &[u8], offset: &mut usize) -> u16 {
    let value = u16::from_le_bytes([bytes[*offset], bytes[*offset + 1]]);
    *offset += 2;
    value
}

fn take_i16(bytes: &[u8], offset: &mut usize) -> i16 {
    take_u16(bytes, offset) as i16
}

fn put_i16(out: &mut [u8], offset: &mut usize, value: i16) {
    put_u16(out, offset, value as u16);
}

fn put_u32(out: &mut [u8], offset: &mut usize, value: u32) {
    out[*offset..*offset + 4].copy_from_slice(&value.to_le_bytes());
    *offset += 4;
}

fn put_i32(out: &mut [u8], offset: &mut usize, value: i32) {
    out[*offset..*offset + 4].copy_from_slice(&value.to_le_bytes());
    *offset += 4;
}

fn put_u64(out: &mut [u8], offset: &mut usize, value: u64) {
    out[*offset..*offset + 8].copy_from_slice(&value.to_le_bytes());
    *offset += 8;
}

#[cfg(test)]
mod tests {
    use std::boxed::Box;

    use super::*;

    const READY: Eligibility = Eligibility {
        thermal_model_ready: true,
        curve_covers_all_targets: true,
        pps_class_available: true,
        idle: true,
        measurement_safe: true,
    };

    #[test]
    fn canonical_target_order_is_frozen() {
        assert_eq!(
            PHYSICAL_TARGETS_C,
            [60, 80, 100, 120, 140, 160, 180, 220, 240]
        );
        assert_eq!(
            EXECUTION_ORDER_C,
            [60, 240, 140, 100, 80, 120, 180, 160, 220]
        );
    }

    #[test]
    fn pps_classes_are_explicit_and_65w_is_3a() {
        assert_eq!(PpsPowerClass::from_str("pps3a"), Some(PpsPowerClass::Pps3a));
        assert_eq!(PpsPowerClass::from_str("pps5a"), Some(PpsPowerClass::Pps5a));
        assert_eq!(PpsPowerClass::from_str("auto"), None);
        assert_eq!(PpsPowerClass::Pps3a.nominal_contract(), (20_000, 3_250));
    }

    #[test]
    fn candidate_hash_is_stable() {
        let profile = CandidateProfile::baseline(PpsPowerClass::Pps3a);
        assert_eq!(
            profile.hash(),
            [
                0xe9, 0x8f, 0xec, 0x4f, 0xfc, 0x42, 0xd3, 0x5d, 0x22, 0x78, 0x65, 0xd4, 0xc2, 0x8a,
                0x55, 0x09, 0xd3, 0xd7, 0x18, 0x43, 0xac, 0x86, 0xda, 0x75, 0x4c, 0xa9, 0x6e, 0xc2,
                0xdb, 0x07, 0x12, 0x9e,
            ]
        );
        assert_ne!(
            profile.hash(),
            CandidateProfile::baseline(PpsPowerClass::Pps5a).hash()
        );
        assert_eq!(
            CandidateProfile::baseline(PpsPowerClass::Pps5a).hash(),
            [
                0xcf, 0x74, 0x33, 0xca, 0x34, 0x20, 0xd9, 0xc7, 0x9a, 0x29, 0xdd, 0xbd, 0xd4, 0x88,
                0xed, 0x65, 0xa3, 0xd0, 0xd0, 0x4a, 0x78, 0x41, 0xc3, 0xe5, 0xa3, 0x69, 0xfb, 0xd6,
                0xd6, 0xab, 0x1e, 0x1e,
            ]
        );
        assert_eq!(
            CandidateIdentity::from_profile(profile).candidate_id,
            profile.hash()[..16]
        );
    }

    #[test]
    fn candidate_profile_canonical_bytes_round_trip_and_reject_invalid_identity() {
        let profile = CandidateProfile::baseline(PpsPowerClass::Pps5a);
        let mut canonical = [0u8; CANDIDATE_PROFILE_CANONICAL_BYTES];
        profile.canonical_bytes(&mut canonical);

        assert_eq!(
            CandidateProfile::from_canonical_bytes(&canonical),
            Some(profile)
        );

        canonical[0] = 4;
        assert_eq!(CandidateProfile::from_canonical_bytes(&canonical), None);

        profile.canonical_bytes(&mut canonical);
        canonical[1] = 61;
        assert_eq!(CandidateProfile::from_canonical_bytes(&canonical), None);
    }

    #[test]
    fn pps5a_high_temperature_seed_preserves_high_heat_margin() {
        let point = CandidatePoint::baseline(240, PpsPowerClass::Pps5a);

        assert_eq!(point.brake_distance_centi_c, 200);
        assert_eq!(point.approach_power_permille, 600);
        assert_eq!(point.approach_floor_power_permille, 450);
        assert_eq!(point.hold_power_permille, 600);
        assert_eq!(point.hold_reheat_power_permille, 600);
        assert_eq!(point.hold_kp_permille_per_c, 40);
        assert_eq!(point.hold_ki_permille_per_c_tick, 4);

        let ladder = candidate_ladder(240, PpsPowerClass::Pps5a);
        assert_eq!(
            ladder.map(|candidate| candidate.hold_power_permille),
            [600, 650, 700]
        );
        assert_eq!(
            ladder.map(|candidate| candidate.approach_power_permille),
            [600, 650, 700]
        );
        assert_eq!(
            ladder.map(|candidate| candidate.brake_distance_centi_c),
            [200, 200, 200]
        );
    }

    #[test]
    fn pps5a_low_temperature_seed_brackets_the_measured_conservative_control() {
        let point = CandidatePoint::baseline(60, PpsPowerClass::Pps5a);

        assert_eq!(point.brake_distance_centi_c, 1_800);
        assert_eq!(point.warmup_power_permille, 750);
        assert_eq!(point.warmup_reenter_centi_c, 1_000);
        assert_eq!(point.approach_power_permille, 450);
        assert_eq!(point.approach_floor_power_permille, 260);
        assert_eq!(point.approach_damping_exponent_permille, 1_800);
        assert_eq!(point.approach_tail_window_centi_c, 0);
        assert_eq!(point.hold_power_permille, 60);
        assert_eq!(point.hold_reheat_power_permille, 60);
        assert_eq!(point.hold_entry_centi_c, 200);
        assert_eq!(point.hold_exit_centi_c, 540);
        assert_eq!(point.hold_off_centi_c, 120);
        assert_eq!(point.overshoot_cutoff_centi_c, 150);
        assert_eq!(point.hold_kp_permille_per_c, 8);
        assert_eq!(point.hold_ki_permille_per_c_tick, 2);
        assert_eq!(point.hold_blend_ticks, 1);
        assert_eq!(point.approach_lead_ticks, 4);
        assert_eq!(point.hold_lead_ticks, 2);

        assert_eq!(
            candidate_ladder(60, PpsPowerClass::Pps5a)
                .map(|candidate| candidate.brake_distance_centi_c),
            [1_800, 2_150, 1_500]
        );
        assert_eq!(
            candidate_ladder(60, PpsPowerClass::Pps5a)
                .map(|candidate| candidate.warmup_power_permille),
            [750, 650, 820]
        );
    }

    #[test]
    fn candidate_ladder_is_bounded_and_uses_canonical_tie_breaker() {
        let ladder = candidate_ladder(140, PpsPowerClass::Pps3a);
        assert_eq!(ladder.len(), CANDIDATE_LADDER_WIDTH);
        assert_ne!(ladder[0], ladder[1]);
        assert_ne!(ladder[1], ladder[2]);
        let gates = CandidateGates {
            warmup_complete: true,
            stage_complete: true,
            overshoot: true,
            hold_peak_to_peak: true,
            hold_confirm: true,
        };
        let evaluations = [
            CandidateEvaluation {
                point: ladder[0],
                score: CandidateScore {
                    max_overshoot_centi: 1,
                    ..CandidateScore::default()
                },
                gates,
            },
            CandidateEvaluation {
                point: ladder[1],
                score: CandidateScore {
                    max_overshoot_centi: 2,
                    ..CandidateScore::default()
                },
                gates,
            },
            CandidateEvaluation {
                point: ladder[2],
                score: CandidateScore {
                    max_overshoot_centi: 3,
                    ..CandidateScore::default()
                },
                gates,
            },
        ];
        assert_eq!(
            select_candidate(140, PpsPowerClass::Pps3a, &evaluations),
            Some(ladder[0])
        );
    }

    #[test]
    fn current_target_seed_interpolates_nearest_accepted_boundaries() {
        let mut core: ThermalTuningCore<16> = ThermalTuningCore::new();
        core.start(7, PpsPowerClass::Pps3a, READY).unwrap();
        core.complete_current_target(TargetDisposition::Accepted, 1, 0, 0, 0, 0, 0, 0, 0, 0)
            .unwrap();
        core.complete_current_target(TargetDisposition::Accepted, 2, 0, 0, 0, 0, 0, 0, 0, 0)
            .unwrap();

        let ladder = core.candidate_ladder_for_current_target().unwrap();
        let baseline = candidate_ladder(140, PpsPowerClass::Pps3a);
        assert_eq!(ladder[0].target_c, 140);
        assert_eq!(ladder[0].brake_distance_centi_c, 694);
        assert_ne!(
            ladder[0].brake_distance_centi_c,
            baseline[0].brake_distance_centi_c
        );

        let gates = CandidateGates {
            warmup_complete: true,
            stage_complete: true,
            overshoot: true,
            hold_peak_to_peak: true,
            hold_confirm: true,
        };
        let evaluations = [
            CandidateEvaluation {
                point: ladder[0],
                score: CandidateScore::default(),
                gates,
            },
            CandidateEvaluation {
                point: ladder[1],
                score: CandidateScore {
                    settle_ms: 1,
                    ..CandidateScore::default()
                },
                gates,
            },
            CandidateEvaluation {
                point: ladder[2],
                score: CandidateScore {
                    settle_ms: 2,
                    ..CandidateScore::default()
                },
                gates,
            },
        ];
        assert_eq!(
            core.freeze_current_target(&evaluations).unwrap(),
            Some(core.candidate_identity().unwrap())
        );
    }

    #[test]
    fn decision_ledger_records_interval_dependencies_and_freeze_state() {
        let mut core = ThermalTuningCore::<8>::new();
        core.start(10, PpsPowerClass::Pps3a, READY).unwrap();
        let gates = CandidateGates {
            warmup_complete: true,
            stage_complete: true,
            overshoot: true,
            hold_peak_to_peak: true,
            hold_confirm: true,
        };
        let ladder = candidate_ladder(60, PpsPowerClass::Pps3a);
        let evaluations = core::array::from_fn(|index| CandidateEvaluation {
            point: ladder[index],
            score: CandidateScore::default(),
            gates,
        });
        core.freeze_current_target(&evaluations).unwrap();
        core.complete_current_target(
            TargetDisposition::Accepted,
            60_000,
            0,
            0,
            0,
            0,
            1_000,
            0,
            0,
            gates.mask(),
        )
        .unwrap();
        core.record_current_decision(TargetDisposition::Failed, 61_000, 0, 0, 0, 0, 0, 0, 0, 0)
            .unwrap();

        let placeholder = TraceRecord {
            sequence: 0,
            event: TraceEvent::Sample(SampleEvent {
                elapsed_ms: 0,
                target_c: 0,
                trial_index: 0,
                candidate_hash: [0; 32],
                temperature_centi_c: 0,
                vin_mv: 0,
                pps_contract_mv: 0,
                pps_contract_ma: 0,
                heater_output_permille: 0,
                measurement_valid: false,
                phase: Phase::Idle,
                heater_phase: HeaterPhase::Warmup,
            }),
            digest: [0; 32],
        };
        let mut records = [placeholder; 8];
        assert_eq!(core.trace_page(None, 8, &mut records), 2);
        match records[0].event {
            TraceEvent::Decision(decision) => {
                assert_eq!(decision.interval_lower_boundary_c, 60);
                assert_eq!(decision.interval_upper_boundary_c, 240);
                assert!(decision.candidate_frozen);
                assert!(!decision.interval_pruned);
            }
            _ => panic!("expected accepted decision"),
        }
        match records[1].event {
            TraceEvent::Decision(decision) => {
                assert_eq!(decision.interval_lower_boundary_c, 60);
                assert_eq!(decision.interval_upper_boundary_c, 240);
                assert!(!decision.candidate_frozen);
                assert!(decision.interval_pruned);
            }
            _ => panic!("expected failed decision"),
        }
    }

    #[test]
    fn failed_target_is_pruned_while_independent_targets_continue() {
        let mut core = ThermalTuningCore::<32>::new();
        core.start(11, PpsPowerClass::Pps3a, READY).unwrap();
        for index in 0..TARGET_COUNT {
            let disposition = if index == 0 {
                TargetDisposition::Failed
            } else {
                TargetDisposition::Accepted
            };
            core.complete_current_target(disposition, index as u32, 0, 0, 0, 0, 0, 0, 0, 0)
                .unwrap();
        }

        let summary = core.summary();
        assert_eq!(summary.state, RunState::Terminal);
        assert_eq!(summary.terminal, Some(TerminalDisposition::Failed));
        assert!(summary.failed[0]);
        assert!(summary.accepted[1..].iter().all(|accepted| *accepted));
        assert_eq!(summary.promotion, PromotionState::Unavailable);
    }

    #[test]
    fn trace_gap_prevents_seal_after_ring_eviction() {
        let mut core = ThermalTuningCore::<2>::new();
        core.start(7, PpsPowerClass::Pps3a, READY).unwrap();
        for index in 0..3 {
            core.record_sample(SampleEvent {
                elapsed_ms: index * 1_000,
                target_c: 60,
                trial_index: 0,
                candidate_hash: [0; 32],
                temperature_centi_c: 6_000,
                vin_mv: 20_000,
                pps_contract_mv: 20_000,
                pps_contract_ma: 3_250,
                heater_output_permille: 500,
                measurement_valid: true,
                phase: Phase::Scout,
                heater_phase: HeaterPhase::Warmup,
            })
            .unwrap();
        }
        assert!(core.trace_gap());
        core.finish(TerminalDisposition::Completed).unwrap();
        assert_eq!(core.seal_review(2, [0; 32]), Err(TraceError::Gap));
    }

    #[test]
    fn repeated_ack_of_the_same_persisted_page_is_idempotent() {
        let mut core = ThermalTuningCore::<4>::new();
        core.start(8, PpsPowerClass::Pps3a, READY).unwrap();
        core.record_sample(SampleEvent {
            elapsed_ms: 0,
            target_c: 60,
            trial_index: 0,
            candidate_hash: [0; 32],
            temperature_centi_c: 2_500,
            vin_mv: 20_000,
            pps_contract_mv: 20_000,
            pps_contract_ma: 3_250,
            heater_output_permille: 0,
            measurement_valid: true,
            phase: Phase::Scout,
            heater_phase: HeaterPhase::Warmup,
        })
        .unwrap();
        let record = core.trace_record(0).unwrap();

        assert_eq!(core.ack_trace(0, record.digest), Ok(()));
        assert_eq!(core.ack_trace(0, record.digest), Ok(()));
        assert_eq!(core.ack_trace(0, [7; 32]), Err(TraceError::DigestMismatch));
    }

    #[test]
    fn large_trace_restart_clears_records_without_reconstructing_the_ring() {
        let mut storage = Box::<ThermalTuningCore<1_024>>::new_uninit();
        unsafe { ThermalTuningCore::init_in_place(storage.as_mut_ptr()) };
        let mut core = unsafe { storage.assume_init() };

        core.start(9, PpsPowerClass::Pps5a, READY).unwrap();
        core.record_sample(SampleEvent {
            elapsed_ms: 0,
            target_c: 60,
            trial_index: 0,
            candidate_hash: [0; 32],
            temperature_centi_c: 2_500,
            vin_mv: 20_000,
            pps_contract_mv: 20_000,
            pps_contract_ma: 5_000,
            heater_output_permille: 0,
            measurement_valid: true,
            phase: Phase::Scout,
            heater_phase: HeaterPhase::Warmup,
        })
        .unwrap();
        core.finish(TerminalDisposition::Cancelled).unwrap();

        core.start(10, PpsPowerClass::Pps5a, READY).unwrap();

        assert_eq!(core.summary().first_sequence, None);
        assert_eq!(core.summary().last_sequence, None);
        assert!(!core.trace_gap());
        assert_eq!(core.trace_record(0), None);
    }

    #[test]
    fn trace_digest_is_stable_for_the_canonical_zero_sample() {
        let mut core = ThermalTuningCore::<8>::new();
        core.start(12, PpsPowerClass::Pps3a, READY).unwrap();
        core.record_sample(SampleEvent {
            elapsed_ms: 0,
            target_c: 0,
            trial_index: 0,
            candidate_hash: [0; 32],
            temperature_centi_c: 0,
            vin_mv: 0,
            pps_contract_mv: 0,
            pps_contract_ma: 0,
            heater_output_permille: 0,
            measurement_valid: false,
            phase: Phase::Idle,
            heater_phase: HeaterPhase::Warmup,
        })
        .unwrap();
        assert_eq!(
            core.trace_digest(),
            [
                0x8e, 0x06, 0xb3, 0x55, 0x7b, 0x7f, 0xe4, 0x85, 0xe4, 0x4a, 0xe5, 0x91, 0x51, 0x25,
                0xc1, 0x9e, 0x49, 0xed, 0xb9, 0x8b, 0x93, 0x01, 0xc9, 0xdc, 0x53, 0x10, 0x1d, 0x52,
                0x8f, 0x57, 0x43, 0xbb,
            ]
        );
    }

    #[test]
    fn completed_run_requires_contiguous_ack_before_seal_and_promotion() {
        let mut core = ThermalTuningCore::<32>::new();
        core.start(8, PpsPowerClass::Pps5a, READY).unwrap();
        core.record_sample(SampleEvent {
            elapsed_ms: 0,
            target_c: 60,
            trial_index: 0,
            candidate_hash: [0; 32],
            temperature_centi_c: 6_000,
            vin_mv: 20_000,
            pps_contract_mv: 20_000,
            pps_contract_ma: 5_000,
            heater_output_permille: 800,
            measurement_valid: true,
            phase: Phase::CooldownWait,
            heater_phase: HeaterPhase::Warmup,
        })
        .unwrap();
        core.finish(TerminalDisposition::Completed).unwrap();
        let digest = core.trace_digest();
        let last = core.summary().last_sequence.unwrap();
        assert_eq!(core.seal_review(last, digest), Ok(()));
        let identity = core.candidate_identity().unwrap();
        assert_eq!(
            core.preview(8, identity, PpsPowerClass::Pps5a),
            Ok(core.candidate().unwrap())
        );
        assert_eq!(
            core.save(8, identity, PpsPowerClass::Pps5a),
            Ok(core.candidate().unwrap())
        );
    }

    #[test]
    fn sealing_accepts_a_terminal_sequence_already_acknowledged_by_the_host() {
        let mut core = ThermalTuningCore::<8>::new();
        core.start(9, PpsPowerClass::Pps3a, READY).unwrap();
        core.record_sample(SampleEvent {
            elapsed_ms: 0,
            target_c: 60,
            trial_index: 0,
            candidate_hash: [0; 32],
            temperature_centi_c: 6_000,
            vin_mv: 20_000,
            pps_contract_mv: 20_000,
            pps_contract_ma: 3_250,
            heater_output_permille: 1_000,
            measurement_valid: true,
            phase: Phase::Scout,
            heater_phase: HeaterPhase::Warmup,
        })
        .unwrap();
        core.finish(TerminalDisposition::Completed).unwrap();
        let last = core.summary().last_sequence.unwrap();
        let digest = core.trace_digest();
        core.ack_trace(last, digest).unwrap();
        assert_eq!(core.seal_review(last, digest), Ok(()));
        assert_eq!(core.promotion(), PromotionState::Ready);
    }
}
