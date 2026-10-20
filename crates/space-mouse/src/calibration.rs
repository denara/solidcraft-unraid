//! Calibration: which of a device's raw axes is which, and how far each one goes. The user holds
//! the cap still, then moves it along each axis both ways; [`Recording::solve`] turns what the
//! device sent into a [`Calibration`] that maps any reading onto 3Dconnexion's axes (X right,
//! Y towards the user, Z down, the rotations about them right-handed), ±[`FULL_SCALE`] at full
//! deflection. Everything after that, the driver mapping and the user's per-axis choices, then
//! treats every device the same.

use crate::FULL_SCALE;

/// One motion the user is asked for: a canonical axis, one way.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Step {
    /// 0..3 = X, Y, Z; 3..6 = rotation about them.
    pub axis: usize,
    pub positive: bool,
}

/// The motions, in the order they are asked for. The positive direction of each is what both
/// 3Dconnexion devices we measured report as positive: slide right, pull towards you, press down,
/// tilt back, tilt left, twist clockwise seen from above.
pub const STEPS: [Step; 12] = {
    let mut steps = [Step { axis: 0, positive: true }; 12];
    let mut i = 0;
    while i < 12 {
        steps[i] = Step { axis: i / 2, positive: i % 2 == 0 };
        i += 1;
    }
    steps
};

/// Where one canonical axis comes from, and how to scale it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AxisCalibration {
    /// The raw axis it is read from.
    pub source: usize,
    /// The raw axis runs the other way.
    pub reversed: bool,
    /// The raw value with the cap at rest.
    pub rest: i16,
    /// How far from `rest` the value wanders with nobody touching the cap; readings within it are 0.
    pub dead_zone: u16,
    /// How far below and above `rest` the value goes at full deflection; each reads as
    /// ±[`FULL_SCALE`], so a device that goes further one way than the other still reaches full
    /// scale both ways.
    pub full: [u16; 2],
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Calibration {
    /// The user's dead zone on top of each axis's measured one, as a share of full deflection
    /// (up to [`MAX_DEAD_ZONE_SHARE`]): readings within it are 0, and the rest of the travel
    /// still reaches full scale. Widening it settles a device that drifts at rest, or whose rest
    /// calibrated off. Calibrations saved before it existed read as 0.
    #[cfg_attr(feature = "serde", serde(default))]
    pub dead_zone_share: f32,
    pub axes: [AxisCalibration; 6],
}

/// The widest dead zone a calibration takes, as a share of full deflection.
pub const MAX_DEAD_ZONE_SHARE: f32 = 0.5;

impl Calibration {
    /// A reading in the device's raw axes, on the canonical ones. Past full deflection the value
    /// keeps growing, up to the i16 range; a source the device doesn't have reads 0.
    pub fn apply(&self, raw: [i16; 6]) -> [i16; 6] {
        let share = if self.dead_zone_share.is_finite() { self.dead_zone_share.clamp(0.0, MAX_DEAD_ZONE_SHARE) } else { 0.0 };
        let shared = (f64::from(share) * f64::from(FULL_SCALE)).round() as i32;
        self.axes.map(|axis| {
            let Some(&value) = raw.get(axis.source) else { return 0 };
            let offset = i32::from(value) - i32::from(axis.rest);
            let dead = i32::from(axis.dead_zone);
            let full = axis.full[usize::from(offset > 0)];
            let span = (i32::from(full) - dead).max(1);
            let past = offset.abs() - dead;
            if past <= 0 {
                return 0;
            }
            // In i64: a short calibrated range and an extreme reading go far past i32 here.
            let full_scale = i64::from(FULL_SCALE);
            let scaled = i64::from(past) * full_scale / i64::from(span);
            // The user's dead zone, in the same units; the travel past it stretches back to full scale.
            if scaled <= i64::from(shared) {
                return 0;
            }
            let scaled = ((scaled - i64::from(shared)) * full_scale / (full_scale - i64::from(shared))).min(i64::from(i16::MAX));
            let signed = if (offset < 0) != axis.reversed { -scaled } else { scaled };
            i16::try_from(signed).unwrap_or(0)
        })
    }

    /// How far each canonical axis is through its travel before any dead zone: ±1 at full
    /// deflection either way, further past it. Where a reading sits on a picture of the axis.
    pub fn travel(&self, raw: [i16; 6]) -> [f32; 6] {
        self.axes.map(|axis| {
            let Some(&value) = raw.get(axis.source) else { return 0.0 };
            let offset = f32::from(value) - f32::from(axis.rest);
            let share = offset / f32::from(axis.full[usize::from(offset > 0.0)]).max(1.0);
            if axis.reversed { -share } else { share }
        })
    }

    /// How much of each canonical axis's travel reads as no motion, `[negative, positive]` as
    /// shares of it: the measured dead zone with the user's on top, as [`Calibration::apply`] has it.
    pub fn dead_zone_shares(&self) -> [[f32; 2]; 6] {
        let user = if self.dead_zone_share.is_finite() { self.dead_zone_share.clamp(0.0, MAX_DEAD_ZONE_SHARE) } else { 0.0 };
        self.axes.map(|axis| {
            let [below, above] = axis.full.map(|full| (f32::from(axis.dead_zone) / f32::from(full).max(1.0)).min(1.0));
            let sides = if axis.reversed { [above, below] } else { [below, above] };
            sides.map(|measured| measured + (1.0 - measured) * user)
        })
    }

    /// Each raw axis's value at rest, which calibrates to no motion: what an app reads a raw axis
    /// as while it has nothing fresh from it. A raw axis no canonical one reads rests at 0.
    pub fn raw_rest(&self) -> [i16; 6] {
        let mut rest = [0; 6];
        for axis in &self.axes {
            if let Some(raw) = rest.get_mut(axis.source) {
                *raw = axis.rest;
            }
        }
        rest
    }

    /// Whether it can be used as it is: each canonical axis reads its own raw axis, and goes
    /// further both ways at full deflection than it wanders at rest. Every calibration
    /// [`Recording::solve`] gives is; one from a hand-edited settings file may not be, and could
    /// turn any touch of the cap into full speed. Its dead zone has to be a share from 0 to
    /// [`MAX_DEAD_ZONE_SHARE`].
    pub fn is_usable(&self) -> bool {
        let mut used = [false; 6];
        (0.0..=MAX_DEAD_ZONE_SHARE).contains(&self.dead_zone_share)
            && self.axes.iter().all(|axis| {
                axis.full.iter().all(|&full| full > axis.dead_zone) && used.get_mut(axis.source).is_some_and(|used| !std::mem::replace(used, true))
            })
    }
}

/// Why a recording didn't give a calibration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Problem {
    /// Nothing was recorded with the cap at rest.
    NoRest,
    /// Nothing was recorded for this step.
    NotRecorded(Step),
    /// Moving the cap along this canonical axis barely moved its raw axis one way: less than twice
    /// what it wanders at rest, or less than [`MIN_SHARE`] of how far any axis went.
    NoResponse(usize),
}

#[derive(Clone, Copy, Debug)]
struct Samples {
    sum: [f64; 6],
    count: u32,
    min: [i16; 6],
    max: [i16; 6],
}

impl Default for Samples {
    fn default() -> Self {
        Self { sum: [0.0; 6], count: 0, min: [i16::MAX; 6], max: [i16::MIN; 6] }
    }
}

impl Samples {
    fn add(&mut self, raw: [i16; 6]) {
        for (((sum, min), max), value) in self.sum.iter_mut().zip(&mut self.min).zip(&mut self.max).zip(raw) {
            *sum += f64::from(value);
            *min = (*min).min(value);
            *max = (*max).max(value);
        }
        self.count += 1;
    }

    fn mean(&self) -> Option<[f64; 6]> {
        (self.count > 0).then(|| self.sum.map(|sum| sum / f64::from(self.count)))
    }

    /// Where raw axis `r` rests (its mean, rounded) and how far its readings went from there;
    /// `None` without readings.
    fn rest_and_wander(&self, r: usize) -> Option<(f64, f64)> {
        let rest = self.mean()?.get(r)?.round();
        let (min, max) = (f64::from(*self.min.get(r)?), f64::from(*self.max.get(r)?));
        Some((rest, (max - rest).max(rest - min).max(0.0)))
    }

    /// How far apart raw axis `r`'s lowest and highest readings were; 0 without readings.
    fn spread(&self, r: usize) -> f64 {
        match (self.min.get(r), self.max.get(r)) {
            (Some(&min), Some(&max)) => (f64::from(max) - f64::from(min)).max(0.0),
            _ => 0.0,
        }
    }
}

/// What the device sent during calibration: readings at rest, during each of [`STEPS`], and
/// where the cap came back to after each one.
#[derive(Clone, Debug, Default)]
pub struct Recording {
    rest: Samples,
    steps: [Samples; 12],
    /// The readings the cap settled on after step `n` was let go: all of a steady run.
    settled: [Samples; 12],
}

impl Recording {
    /// One reading with nobody touching the cap.
    pub fn rest(&mut self, raw: [i16; 6]) {
        self.rest.add(raw);
    }

    /// One reading during step `index` of [`STEPS`]; any other index is ignored.
    pub fn step(&mut self, index: usize, raw: [i16; 6]) {
        if let Some(samples) = self.steps.get_mut(index) {
            samples.add(raw);
        }
    }

    /// One reading of where the cap settled after step `index` was let go; any other index is ignored.
    pub fn settled(&mut self, index: usize, raw: [i16; 6]) {
        if let Some(settled) = self.settled.get_mut(index) {
            settled.add(raw);
        }
    }

    /// Whether step `index` has a reading yet.
    pub fn has_step(&self, index: usize) -> bool {
        self.steps.get(index).is_some_and(|samples| samples.count > 0)
    }

    /// Forgets what was recorded for step `index`.
    fn clear_step(&mut self, index: usize) {
        if let Some(samples) = self.steps.get_mut(index) {
            *samples = Samples::default();
        }
        if let Some(settled) = self.settled.get_mut(index) {
            *settled = Samples::default();
        }
    }

    /// The calibration the recording gives. Hands are never exact: sliding the cap tilts it a
    /// little and tilting it slides it, sometimes further than the axis being asked for. So the
    /// raw axis for each canonical one isn't the one that moved most in its step. It is the
    /// one-to-one assignment that best explains all six axes together, scored by how far each
    /// raw axis moved between the step's two directions, as a share of the furthest it moved in
    /// any step: an axis with a wider range doesn't win through cross-talk alone. A raw axis that
    /// only wandered scores low, since its share is of at least four times its wander at rest.
    ///
    /// A device that doesn't come back to the same place after each release (a drifting stick, or
    /// spacenavd keeping a relative-axis device's last small report, #2778) rests, here, at the
    /// middle of everything it read at rest and after each release, and its dead zone covers all of
    /// it, so where it stops doesn't creep.
    pub fn solve(&self) -> Result<Calibration, Problem> {
        let rest = self.rest.mean().ok_or(Problem::NoRest)?;
        let mut means = [[0.0; 6]; 12];
        for ((mean, samples), step) in means.iter_mut().zip(&self.steps).zip(STEPS) {
            *mean = samples.mean().ok_or(Problem::NotRecorded(step))?;
        }
        // travel[c][r]: how far raw axis r moved from canonical axis c's negative step to its positive one.
        let travel: [[f64; 6]; 6] = std::array::from_fn(|c| std::array::from_fn(|r| means[2 * c][r] - means[2 * c + 1][r]));
        let widest: [f64; 6] = std::array::from_fn(|r| travel.iter().map(|row| row[r].abs()).fold(4.0 * self.rest.spread(r), f64::max).max(1.0));
        let shares: [[f64; 6]; 6] = std::array::from_fn(|c| std::array::from_fn(|r| travel[c][r].abs() / widest[r]));
        let sources = best_assignment(&shares);
        let mut axes = [AxisCalibration { source: 0, reversed: false, rest: 0, dead_zone: 0, full: [0; 2] }; 6];
        for (c, (axis, source)) in axes.iter_mut().zip(sources).enumerate() {
            let (rest_value, wander) = self.rest.rest_and_wander(source).unwrap_or((rest[source].round(), 0.0));
            let (rest_value, wander) = self.settled_rest(source, rest_value, wander);
            let (positive, negative) = (&self.steps[2 * c], &self.steps[2 * c + 1]);
            let reversed = travel[c][source] < 0.0;
            // The steps that push the raw axis above and below rest, and how far each got.
            let (above, below) = if reversed { (negative, positive) } else { (positive, negative) };
            let full = [rest_value - f64::from(below.min[source]), f64::from(above.max[source]) - rest_value];
            if full.iter().any(|&full| full < 2.0 * wander || full < 1.0) {
                return Err(Problem::NoResponse(c));
            }
            *axis = AxisCalibration {
                source,
                reversed,
                rest: rest_value.clamp(f64::from(i16::MIN), f64::from(i16::MAX)) as i16,
                dead_zone: wander.min(f64::from(u16::MAX)) as u16,
                full: full.map(|full| full.min(f64::from(u16::MAX)) as u16),
            };
        }
        let most = axes.iter().flat_map(|axis| axis.full).max().unwrap_or(0);
        if let Some(c) = axes.iter().position(|axis| axis.full.iter().any(|&full| f64::from(full) < MIN_SHARE * f64::from(most))) {
            return Err(Problem::NoResponse(c));
        }
        Ok(Calibration { dead_zone_share: 0.0, axes })
    }
}

impl Recording {
    /// Raw axis `r`'s rest and dead zone, given the rest stage's: unchanged while every reading
    /// after a release stayed within the wander; otherwise the middle of everything read at rest
    /// and after the releases, with a dead zone reaching every one of those readings.
    fn settled_rest(&self, r: usize, rest: f64, wander: f64) -> (f64, f64) {
        let (mut low, mut high) = (rest - wander, rest + wander);
        for settled in self.settled.iter().filter(|settled| settled.count > 0) {
            if let (Some(&min), Some(&max)) = (settled.min.get(r), settled.max.get(r)) {
                (low, high) = (low.min(f64::from(min)), high.max(f64::from(max)));
            }
        }
        if low >= rest - wander && high <= rest + wander {
            return (rest, wander);
        }
        let middle = ((low + high) / 2.0).round();
        (middle, (high - middle).max(middle - low))
    }
}

/// The raw axis for each canonical one, each raw axis used once, with the highest total share.
fn best_assignment(shares: &[[f64; 6]; 6]) -> [usize; 6] {
    fn search(shares: &[[f64; 6]; 6], canonical: usize, used: [bool; 6], current: &mut [usize; 6], total: f64, best: &mut ([usize; 6], f64)) {
        if canonical == 6 {
            if total > best.1 {
                *best = (*current, total);
            }
            return;
        }
        for raw in 0..6 {
            if !used[raw] {
                let mut now_used = used;
                now_used[raw] = true;
                current[canonical] = raw;
                search(shares, canonical + 1, now_used, current, total + shares[canonical][raw], best);
            }
        }
    }
    let mut best = ([0, 1, 2, 3, 4, 5], f64::NEG_INFINITY);
    search(shares, 0, [false; 6], &mut [0; 6], 0.0, &mut best);
    best.0
}

/// The least a direction may go, as a share of how far the furthest axis went, before it counts
/// as not having responded. It turns away a step that recorded a resting hand rather than a push,
/// which would otherwise scale a few units of noise up to full speed, and still lets a homemade
/// device go ten times further one way than the other.
pub const MIN_SHARE: f64 = 1.0 / 12.0;

/// Seconds of readings with the cap at rest before the first step.
pub const REST_SECONDS: f64 = 1.5;
/// Seconds the cap has to be held out in a step's direction before letting go finishes it.
pub const HOLD_SECONDS: f64 = 0.4;
/// Seconds the reading has to stay steady, back close to rest, to count as let go even outside the
/// rest wander: the cap stopped short of rest, as a drifting stick does, or spacenavd with a
/// relative-axis device (#2778). Back close to rest means the axis that went furthest this step is
/// at least [`RETURNED_SHARE`] of the way back, and some axis that moved about as far (at least
/// half as far) is within [`SETTLED_SHARE`] of its own peak: cross-talk can move another raw axis
/// further than the one being asked for, and which raw axis that is isn't known yet. Steady means
/// each raw axis's readings span no more than twice its rest wander, at least 4 units (±2) and
/// [`STEADY_SHARE`] of the furthest that axis has gone in any step: an axis that read flat at rest
/// may still dither once it's off-centre.
pub const SETTLE_SECONDS: f64 = 0.5;
/// How far from rest, as a share of its peak this step, an axis that moved has to be to count as let go.
pub const SETTLED_SHARE: f64 = 0.25;
/// How far from rest, as a share of its peak, the axis that went furthest may still be when the
/// step counts as let go. A hand still holding the push keeps it out.
pub const RETURNED_SHARE: f64 = 0.5;
/// The least spread steady readings may have, as a share of the furthest the axis has gone.
pub const STEADY_SHARE: f64 = 1.0 / 50.0;

/// Where the guided calibration is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    /// Hands off the cap.
    Rest,
    /// Step `n` of [`STEPS`].
    Step(usize),
    /// Every step is recorded.
    Done,
}

/// The guided calibration: it records the cap at rest, then each of [`STEPS`] in turn, and moves
/// on by itself, so the user's hand stays on the cap. A step is done once the cap has been held
/// out for [`HOLD_SECONDS`] in one go and let go. Before calibration nothing says how far a raw
/// axis goes, so "held out" only means some raw axis is at least twice as far from rest as it
/// wandered at rest, each axis measured against its own wander: the least [`Recording::solve`]
/// accepts. So whatever the solver would accept can be recorded, a noisy axis doesn't hold up a
/// quiet one, and the solver judges whether each direction went far enough. "Let go" means every
/// raw axis is back within its rest wander, or the reading has gone steady close to rest
/// ([`SETTLE_SECONDS`], [`SETTLED_SHARE`]).
#[derive(Clone, Debug)]
pub struct Wizard {
    stage: Stage,
    recording: Recording,
    /// Seconds spent in the current stage: at rest, or held out.
    seconds: f64,
    /// The dead zone the user set while calibrating, which the result carries.
    dead_zone_share: f32,
    /// The readings since the reading went steady, and for how long.
    run: Samples,
    steady: f64,
    /// How far each raw axis went from rest in this step, in raw units.
    peaks: [f64; 6],
    /// The furthest each raw axis went from rest in any step so far.
    reach: [f64; 6],
    /// The readings the previous step settled on: while a reading stays within their spread, nobody has touched the cap.
    settled: Option<Samples>,
}

impl Default for Wizard {
    fn default() -> Self {
        Self {
            stage: Stage::Rest,
            recording: Recording::default(),
            seconds: 0.0,
            dead_zone_share: 0.0,
            run: Samples::default(),
            steady: 0.0,
            peaks: [0.0; 6],
            reach: [0.0; 6],
            settled: None,
        }
    }
}

impl Wizard {
    pub fn stage(&self) -> Stage {
        self.stage
    }

    /// How far through the current stage, 0..=1: the time at rest, or held out.
    pub fn progress(&self) -> f64 {
        match self.stage {
            Stage::Rest => (self.seconds / REST_SECONDS).min(1.0),
            Stage::Step(_) => (self.seconds / HOLD_SECONDS).min(1.0),
            Stage::Done => 1.0,
        }
    }

    /// One reading, `seconds` after the previous one.
    pub fn feed(&mut self, raw: [i16; 6], seconds: f64) {
        // A stalled frame shouldn't finish a stage on its own.
        let seconds = if seconds.is_finite() { seconds.clamp(0.0, 0.1) } else { 0.0 };
        // Steady while every axis's readings since the run began stay within its noise.
        let mut run = self.run;
        run.add(raw);
        if (0..6).all(|r| run.spread(r) <= self.tolerance(r)) {
            self.steady += seconds;
        } else {
            (run, self.steady) = (Samples::default(), 0.0);
            run.add(raw);
        }
        self.run = run;
        match self.stage {
            Stage::Rest => {
                self.recording.rest(raw);
                self.seconds += seconds;
                if self.seconds >= REST_SECONDS {
                    self.enter(Stage::Step(0));
                }
            }
            Stage::Step(index) => {
                // Still where the last step settled: the cap hasn't been touched yet. Deliberately
                // skips the peak and hold bookkeeping; a device that rests exactly there reads as
                // untouched until it moves, which is what it is.
                if self.settled.is_some_and(|settled| self.near(settled, raw)) {
                    return;
                }
                self.settled = None;
                let away = self.away(raw);
                let offsets = self.offsets(raw);
                for ((peak, reach), offset) in self.peaks.iter_mut().zip(&mut self.reach).zip(offsets) {
                    *peak = peak.max(offset);
                    *reach = reach.max(offset);
                }
                let furthest = self.peaks.iter().copied().fold(0.0, f64::max);
                let peaks = self.peaks;
                // Close to rest is the cap settling, never held; it has let go once it's steady there.
                // Every axis that moved about as far as the furthest has to be back by
                // `RETURNED_SHARE` of its own peak too, or a lesser axis still held out would be
                // missed while a larger cross-talk axis relaxes.
                let settling = (0..6).any(|r| peaks[r] == furthest && offsets[r] <= RETURNED_SHARE * furthest)
                    && (0..6).any(|r| peaks[r] >= 0.5 * furthest && offsets[r] <= SETTLED_SHARE * peaks[r])
                    && (0..6).all(|r| peaks[r] < 0.5 * furthest || offsets[r] <= RETURNED_SHARE * peaks[r]);
                let stopped = settling && (self.steady >= SETTLE_SECONDS);
                if away <= 0.5 || stopped {
                    // Where it settled: every reading of the steady run, or just this one.
                    let mut settled = Samples::default();
                    settled.add(raw);
                    if stopped {
                        settled = self.run;
                    }
                    if self.seconds >= HOLD_SECONDS {
                        // Every reading of the steady run, so the dead zone covers all of them.
                        if stopped {
                            self.recording.settled[index] = self.run;
                        } else {
                            self.recording.settled(index, raw);
                        }
                        self.enter(if index + 1 < STEPS.len() { Stage::Step(index + 1) } else { Stage::Done });
                        self.settled = Some(settled);
                    } else if self.seconds > 0.0 {
                        // Let go too soon: the hold starts over, and only a whole one is kept.
                        self.recording.clear_step(index);
                        self.enter(Stage::Step(index));
                        self.settled = Some(settled);
                    }
                } else if away >= 1.0 && !settling {
                    self.recording.step(index, raw);
                    self.seconds += seconds;
                }
            }
            Stage::Done => {}
        }
    }

    /// Goes back one step and records it again; from the first step, back to rest.
    pub fn back(&mut self) {
        match self.stage {
            Stage::Rest => {}
            Stage::Step(0) => *self = Self::default(),
            Stage::Step(index) => {
                self.recording.clear_step(index);
                self.recording.clear_step(index - 1);
                self.enter(Stage::Step(index - 1));
            }
            Stage::Done => {
                self.recording.clear_step(STEPS.len() - 1);
                self.enter(Stage::Step(STEPS.len() - 1));
            }
        }
    }

    /// Once every step is recorded, the calibration they give, with the dead zone set here.
    pub fn result(&self) -> Option<Result<Calibration, Problem>> {
        (self.stage == Stage::Done)
            .then(|| self.recording.solve().map(|calibration| Calibration { dead_zone_share: self.dead_zone_share, ..calibration }))
    }

    pub fn dead_zone_share(&self) -> f32 {
        self.dead_zone_share
    }

    /// Sets the dead zone the result carries, a share from 0 to [`MAX_DEAD_ZONE_SHARE`]; anything
    /// else is held to that range, and a value that isn't a number reads as 0.
    pub fn set_dead_zone_share(&mut self, share: f32) {
        self.dead_zone_share = if share.is_finite() { share.clamp(0.0, MAX_DEAD_ZONE_SHARE) } else { 0.0 };
    }

    fn enter(&mut self, stage: Stage) {
        self.stage = stage;
        self.seconds = 0.0;
        self.peaks = [0.0; 6];
        // A new stage's steadiness starts with its own readings.
        (self.run, self.steady) = (Samples::default(), 0.0);
    }

    /// How far each raw axis is from rest, in raw units.
    fn offsets(&self, raw: [i16; 6]) -> [f64; 6] {
        std::array::from_fn(|r| (f64::from(raw[r]) - self.recording.rest.rest_and_wander(r).map_or(0.0, |(rest, _)| rest)).abs())
    }

    /// How far apart raw axis `r`'s readings may be and still be one position: its noise's whole
    /// spread (twice its rest wander), at least 4 units (±2) and [`STEADY_SHARE`] of the furthest it
    /// has gone in any step.
    fn tolerance(&self, r: usize) -> f64 {
        let wander = self.recording.rest.rest_and_wander(r).map_or(0.0, |(_, wander)| wander);
        (2.0 * wander).max(4.0).max(STEADY_SHARE * self.reach.get(r).copied().unwrap_or(0.0))
    }

    /// Whether `raw` is still where `settled` was: adding it keeps every axis within [`Self::tolerance`],
    /// the same band the steadiness check accepts.
    fn near(&self, mut settled: Samples, raw: [i16; 6]) -> bool {
        settled.add(raw);
        (0..6).all(|r| settled.spread(r) <= self.tolerance(r))
    }

    /// How far the reading is from rest, on the raw axis furthest out, as a share of what counts as
    /// held out for that axis (twice its rest wander, at least one unit): 1 or more is held out,
    /// half or less is back within its wander.
    fn away(&self, raw: [i16; 6]) -> f64 {
        raw.iter()
            .enumerate()
            .map(|(r, &value)| {
                let (rest, wander) = self.recording.rest.rest_and_wander(r).unwrap_or((0.0, 0.0));
                (f64::from(value) - rest).abs() / (2.0 * wander).max(1.0)
            })
            .fold(0.0, f64::max)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The mean of each axis (X Y Z Rx Ry Rz) during each step, in `STEPS` order, as measured
    /// with our own SpaceMouse Pro (046d:c62b) following capture prompts. Note the slide right
    /// row: Ry moved further than X.
    const PRO: [[i16; 6]; 12] = [
        [250, -10, 0, 0, -336, 0],     // slide right
        [-277, 55, 86, -36, -196, 0],  // slide left
        [0, 324, 82, -26, -3, -28],    // pull towards you
        [-2, -340, 219, -26, -20, 0],  // push away
        [-1, -3, 344, 0, -37, 0],      // press down
        [41, -24, -341, -11, -10, 32], // lift up
        [0, 193, 0, 333, -26, 0],      // tilt back
        [0, -339, 163, -183, 0, 0],    // tilt forward
        [61, 0, 1, -12, 328, 0],       // tilt left
        [9, 80, 1, 0, -330, -6],       // tilt right
        [0, -1, 78, 0, 0, 341],        // twist clockwise
        [0, -6, 0, 0, -4, -337],       // twist counterclockwise
    ];

    /// The same with our own SpaceMouse Wireless (256f:c63a), all six axes in one report.
    const WIRELESS: [[i16; 6]; 12] = [
        [325, 92, 72, 0, 44, -6],
        [-336, 38, 104, 19, -66, 5],
        [-5, 339, 73, -71, 0, 0],
        [-18, -323, 144, -2, 1, 0],
        [0, 1, 341, -2, 2, 0],
        [0, 1, -320, 1, -43, 0],
        [0, -12, 3, 330, 4, -24],
        [0, 51, 17, -319, -20, 0],
        [-23, 0, 0, -1, 333, 0],
        [7, 41, 23, 15, -163, -13],
        [-13, 4, 44, -12, -12, 326],
        [0, -1, -1, 1, -9, -313],
    ];

    fn record(steps: [[i16; 6]; 12], rest: [[i16; 6]; 2]) -> Recording {
        let mut recording = Recording::default();
        for reading in rest {
            recording.rest(reading);
        }
        for (index, reading) in steps.into_iter().enumerate() {
            recording.step(index, reading);
        }
        recording
    }

    const STILL: [[i16; 6]; 2] = [[-3, 2, 0, 1, -2, 0], [3, -2, 0, -1, 2, 0]];

    #[test]
    fn both_3dconnexion_devices_calibrate_to_their_own_axes() {
        for steps in [PRO, WIRELESS] {
            let calibration = record(steps, STILL).solve().unwrap();
            for (c, axis) in calibration.axes.iter().enumerate() {
                assert_eq!((axis.source, axis.reversed), (c, false), "axis {c}");
                assert!(axis.full.iter().all(|full| (150..=350).contains(full)), "axis {c}: {axis:?}");
            }
        }
    }

    #[test]
    fn a_homemade_device_with_shuffled_reversed_wider_axes_is_found() {
        // A device reporting ±32767, its axes in another order (raw r carries canonical
        // ORDER[r]), the odd ones running the other way, at rest around 100.
        const ORDER: [usize; 6] = [4, 0, 5, 2, 1, 3];
        let wire = |canonical: [i16; 6]| -> [i16; 6] {
            std::array::from_fn(|r| {
                let v = i32::from(canonical[ORDER[r]]) * 32767 / 350;
                i16::try_from(if r % 2 == 1 { -v } else { v } + 100).unwrap()
            })
        };
        let calibration = record(PRO.map(wire), STILL.map(wire)).solve().unwrap();
        for (c, axis) in calibration.axes.iter().enumerate() {
            let r = ORDER.iter().position(|&o| o == c).unwrap();
            assert_eq!((axis.source, axis.reversed), (r, r % 2 == 1), "axis {c}");
            assert_eq!(axis.rest, 100);
        }
        // Each step now reads on its own axis, the right way, at full scale: each direction is
        // scaled on its own, so the Pro's shorter tilt forward gets there too.
        for (step, reading) in STEPS.iter().zip(PRO.map(wire)) {
            let value = calibration.apply(reading)[step.axis];
            assert_eq!(value, if step.positive { FULL_SCALE } else { -FULL_SCALE }, "{step:?}");
        }
    }

    #[test]
    fn readings_within_the_rest_wander_are_zero_and_full_deflection_is_full_scale() {
        let axis = AxisCalibration { source: 1, reversed: true, rest: 10, dead_zone: 5, full: [105; 2] };
        let calibration = Calibration { dead_zone_share: 0.0, axes: [axis; 6] };
        assert_eq!(calibration.apply([0, 14, 0, 0, 0, 0]), [0; 6]);
        assert_eq!(calibration.apply([0, 115, 0, 0, 0, 0]), [-FULL_SCALE; 6]);
        assert_eq!(calibration.apply([0, -95, 0, 0, 0, 0]), [FULL_SCALE; 6]);
        // Past full deflection the value keeps going, up to the i16 range.
        assert_eq!(calibration.apply([0, i16::MIN, 0, 0, 0, 0]), [i16::MAX; 6]);
    }

    #[test]
    fn a_device_that_goes_further_one_way_reaches_full_scale_both_ways() {
        let axis = AxisCalibration { source: 0, reversed: false, rest: 0, dead_zone: 0, full: [200, 500] };
        let calibration = Calibration { dead_zone_share: 0.0, axes: [axis; 6] };
        assert_eq!(calibration.apply([500, 0, 0, 0, 0, 0]), [FULL_SCALE; 6]);
        assert_eq!(calibration.apply([-200, 0, 0, 0, 0, 0]), [-FULL_SCALE; 6]);
        assert_eq!(calibration.apply([-100, 0, 0, 0, 0, 0]), [-FULL_SCALE / 2; 6]);
    }

    #[test]
    fn the_users_dead_zone_comes_off_the_bottom_and_full_deflection_stays_full_scale() {
        let axis = AxisCalibration { source: 0, reversed: false, rest: 0, dead_zone: 0, full: [350; 2] };
        let calibration = Calibration { dead_zone_share: 0.2, axes: [axis; 6] };
        assert_eq!(calibration.apply([70, 0, 0, 0, 0, 0])[0], 0, "a fifth of the travel is inside it");
        assert_eq!(calibration.apply([210, 0, 0, 0, 0, 0])[0], 175, "60% of the travel is halfway past it");
        assert_eq!(calibration.apply([-350, 0, 0, 0, 0, 0])[0], -FULL_SCALE);
        // Out of range or not a number: held to the range, or none.
        assert_eq!(Calibration { dead_zone_share: 9.0, ..calibration }.apply([170, 0, 0, 0, 0, 0])[0], 0);
        assert_eq!(Calibration { dead_zone_share: f32::NAN, ..calibration }.apply([70, 0, 0, 0, 0, 0])[0], 70);
        let solved = record(PRO, STILL).solve().unwrap();
        assert!(Calibration { dead_zone_share: 0.5, ..solved }.is_usable());
        assert!(!Calibration { dead_zone_share: 0.6, ..solved }.is_usable());
        assert!(!Calibration { dead_zone_share: f32::NAN, ..solved }.is_usable());
        assert!(!Calibration { dead_zone_share: -0.1, ..solved }.is_usable());
    }

    #[test]
    fn extreme_readings_on_a_short_range_saturate_with_the_right_sign() {
        // Six distinct sources, each with a range of one unit: any real reading is far past it.
        let axes: [AxisCalibration; 6] = std::array::from_fn(|c| AxisCalibration {
            source: (c + 2) % 6,
            reversed: c % 2 == 1,
            rest: if c < 3 { 0 } else { i16::MIN },
            dead_zone: 0,
            full: [1, 1],
        });
        for dead_zone_share in [0.0, MAX_DEAD_ZONE_SHARE] {
            let calibration = Calibration { dead_zone_share, axes };
            assert!(calibration.is_usable());
            let read = |raw: [i16; 6]| calibration.apply(raw);
            for (raw, sign) in [(i16::MAX, 1), (i16::MIN, -1)] {
                let out = read([raw; 6]);
                for (c, axis) in calibration.axes.iter().enumerate() {
                    let offset = i32::from(raw) - i32::from(axis.rest);
                    let expected = if offset == 0 {
                        0
                    } else {
                        let positive = (offset > 0) != axis.reversed;
                        if positive { i16::MAX } else { -i16::MAX }
                    };
                    assert_eq!(out[c], expected, "share {dead_zone_share}, raw {raw} (sign {sign}), axis {c}: {axis:?}");
                }
            }
        }
    }

    #[test]
    fn the_dead_zone_set_while_calibrating_goes_with_the_result() {
        let mut wizard = Wizard::default();
        wizard.set_dead_zone_share(0.15);
        rest_then(&mut wizard);
        for reading in PRO {
            hold(&mut wizard, reading, 0.5);
            hold(&mut wizard, STILL[0], 0.1);
        }
        assert_eq!(wizard.result().unwrap().unwrap().dead_zone_share, 0.15);
        wizard.set_dead_zone_share(f32::NAN);
        assert_eq!(wizard.dead_zone_share(), 0.0);
        wizard.set_dead_zone_share(2.0);
        assert_eq!(wizard.dead_zone_share(), MAX_DEAD_ZONE_SHARE);
    }

    #[test]
    fn a_device_at_its_raw_rest_reads_no_motion() {
        let axis = |source, rest| AxisCalibration { source, reversed: source % 2 == 1, rest, dead_zone: 0, full: [350; 2] };
        let calibration =
            Calibration { dead_zone_share: 0.0, axes: [axis(3, 100), axis(0, -40), axis(1, 7), axis(2, 0), axis(5, 1000), axis(4, -1)] };
        assert_eq!(calibration.raw_rest(), [-40, 7, 0, 100, -1, 1000]);
        assert_eq!(calibration.apply(calibration.raw_rest()), [0; 6]);
    }

    #[test]
    fn a_source_the_device_doesnt_have_reads_zero() {
        let axis = AxisCalibration { source: 9, reversed: false, rest: 0, dead_zone: 0, full: [350; 2] };
        assert_eq!(Calibration { dead_zone_share: 0.0, axes: [axis; 6] }.apply([350; 6]), [0; 6]);
    }

    #[test]
    fn a_missing_step_or_rest_is_named() {
        let mut recording = record(PRO, STILL);
        recording.steps[7] = Samples::default();
        assert_eq!(recording.solve(), Err(Problem::NotRecorded(STEPS[7])));
        assert!(!recording.has_step(7) && recording.has_step(6));
        assert_eq!(Recording::default().solve(), Err(Problem::NoRest));
    }

    #[test]
    fn an_axis_that_barely_moves_is_named() {
        let mut steps = PRO;
        // Twisting did nothing beyond what the cap wanders at rest.
        steps[10] = [0, 0, 0, 0, 0, 20];
        steps[11] = [0, 0, 0, 0, 0, -20];
        assert_eq!(record(steps, STILL).solve(), Err(Problem::NoResponse(5)));
    }

    /// Feeds `reading` for `seconds`, a frame every 20 ms.
    fn hold(wizard: &mut Wizard, reading: [i16; 6], seconds: f64) {
        for _ in 0..(seconds / 0.02).round() as usize {
            wizard.feed(reading, 0.02);
        }
    }

    fn rest_then(wizard: &mut Wizard) {
        for _ in 0..40 {
            hold(wizard, STILL[0], 0.02);
            hold(wizard, STILL[1], 0.02);
        }
    }

    #[test]
    fn the_wizard_moves_on_by_itself_and_ends_with_the_calibration() {
        let mut wizard = Wizard::default();
        rest_then(&mut wizard);
        for (index, reading) in PRO.into_iter().enumerate() {
            assert_eq!(wizard.stage(), Stage::Step(index));
            hold(&mut wizard, reading, 0.5);
            assert_eq!(wizard.progress(), 1.0);
            hold(&mut wizard, STILL[0], 0.1);
        }
        assert_eq!(wizard.stage(), Stage::Done);
        let calibration = wizard.result().unwrap().unwrap();
        assert!(calibration.axes.iter().enumerate().all(|(c, axis)| axis.source == c && !axis.reversed));
    }

    #[test]
    fn a_quick_twitch_or_a_hold_without_letting_go_doesnt_finish_a_step() {
        let mut wizard = Wizard::default();
        rest_then(&mut wizard);
        hold(&mut wizard, PRO[0], 0.2);
        hold(&mut wizard, STILL[0], 0.1);
        assert_eq!(wizard.stage(), Stage::Step(0));
        hold(&mut wizard, PRO[0], 2.0);
        assert_eq!(wizard.stage(), Stage::Step(0));
        // A stalled frame counts as a short one.
        let mut stalled = Wizard::default();
        rest_then(&mut stalled);
        stalled.feed(PRO[0], 5.0);
        assert!(stalled.progress() < 1.0);
    }

    #[test]
    fn back_records_the_previous_step_again() {
        let mut wizard = Wizard::default();
        rest_then(&mut wizard);
        for reading in &PRO[..2] {
            hold(&mut wizard, *reading, 0.5);
            hold(&mut wizard, STILL[0], 0.1);
        }
        assert_eq!(wizard.stage(), Stage::Step(2));
        wizard.back();
        assert_eq!(wizard.stage(), Stage::Step(1));
        assert!(wizard.recording.has_step(0) && !wizard.recording.has_step(1));
        wizard.back();
        wizard.back();
        assert_eq!(wizard.stage(), Stage::Rest);
        assert!(wizard.result().is_none());
    }

    #[test]
    fn a_device_whose_rotations_go_a_fifth_as_far_as_its_slides_finishes_and_calibrates() {
        let weak = |reading: [i16; 6]| -> [i16; 6] { std::array::from_fn(|r| if r < 3 { reading[r] } else { reading[r] / 5 }) };
        let mut wizard = Wizard::default();
        rest_then(&mut wizard);
        for (index, reading) in PRO.map(weak).into_iter().enumerate() {
            assert_eq!(wizard.stage(), Stage::Step(index));
            hold(&mut wizard, reading, 0.5);
            hold(&mut wizard, STILL[0], 0.1);
        }
        let calibration = wizard.result().unwrap().unwrap();
        assert!(calibration.axes.iter().enumerate().all(|(c, axis)| axis.source == c && !axis.reversed), "{calibration:?}");
    }

    #[test]
    fn two_short_nudges_dont_add_up_to_a_hold() {
        let mut wizard = Wizard::default();
        rest_then(&mut wizard);
        for _ in 0..2 {
            hold(&mut wizard, PRO[0], 0.24);
            hold(&mut wizard, STILL[0], 0.1);
        }
        assert_eq!(wizard.stage(), Stage::Step(0));
        assert_eq!(wizard.progress(), 0.0);
        assert!(!wizard.recording.has_step(0));
    }

    #[test]
    fn a_device_that_goes_ten_times_further_one_way_finishes_and_calibrates() {
        // Slide right goes to +500, slide left only to -50; the other axes as the Pro.
        let mut steps = PRO;
        steps[0][0] = 500;
        steps[1][0] = -50;
        let mut wizard = Wizard::default();
        rest_then(&mut wizard);
        for (index, reading) in steps.into_iter().enumerate() {
            assert_eq!(wizard.stage(), Stage::Step(index));
            hold(&mut wizard, reading, 0.5);
            hold(&mut wizard, STILL[0], 0.1);
        }
        let calibration = wizard.result().unwrap().unwrap();
        assert_eq!((calibration.axes[0].source, calibration.axes[0].full), (0, [50, 500]));
        assert_eq!(calibration.apply([-50, 0, 0, 0, 0, 0])[0], -FULL_SCALE);
    }

    /// Runs the whole wizard: `rest` alternating for the rest stage, then each step held and let
    /// go back to `rest[0]`.
    fn walk(rest: [[i16; 6]; 2], steps: [[i16; 6]; 12]) -> Wizard {
        let mut wizard = Wizard::default();
        for _ in 0..40 {
            hold(&mut wizard, rest[0], 0.02);
            hold(&mut wizard, rest[1], 0.02);
        }
        for (index, reading) in steps.into_iter().enumerate() {
            assert_eq!(wizard.stage(), Stage::Step(index), "{rest:?}");
            hold(&mut wizard, reading, 0.5);
            hold(&mut wizard, rest[0], 0.1);
        }
        wizard
    }

    /// Where a relative-axis device read through spacenavd stops after each release: its last
    /// small report, never zero (#2778). Ours, in the range that issue measured.
    const STUCK: [[i16; 6]; 13] = [
        [11, -34, -40, -5, -5, 15],
        [0, 0, -60, 0, 0, 0],
        [-8, 21, 13, 4, -9, -3],
        [25, -6, 0, -12, 7, 2],
        [-30, 12, 9, 0, 18, -7],
        [4, 38, -15, 6, -2, 0],
        [0, -17, 33, -20, 1, 9],
        [9, 3, -4, 28, -11, 0],
        [-2, -9, 6, -3, 24, -12],
        [14, 0, -11, 7, -30, 5],
        [-6, 10, 2, 0, 3, 31],
        [3, -4, 0, 2, -6, -27],
        [0, 0, 0, 0, 0, 0],
    ];

    #[test]
    fn a_device_that_stops_short_of_rest_after_each_release_finishes_and_doesnt_creep() {
        // Nothing arrives at rest, so the reading stays exactly where the last report left it:
        // first wherever the cap was last let go, then after each step a different place.
        let mut wizard = Wizard::default();
        hold(&mut wizard, STUCK[0], REST_SECONDS + 0.1);
        for (index, reading) in PRO.into_iter().enumerate() {
            assert_eq!(wizard.stage(), Stage::Step(index));
            // While the cap hasn't been touched, the last step's stuck reading isn't a hold.
            hold(&mut wizard, STUCK[index], 1.0);
            assert_eq!((wizard.stage(), wizard.progress()), (Stage::Step(index), 0.0));
            hold(&mut wizard, reading, 0.5);
            hold(&mut wizard, STUCK[index + 1], SETTLE_SECONDS + 0.1);
        }
        assert_eq!(wizard.stage(), Stage::Done);
        let calibration = wizard.result().unwrap().unwrap();
        assert!(calibration.axes.iter().enumerate().all(|(c, axis)| axis.source == c && !axis.reversed), "{calibration:?}");
        for stuck in STUCK {
            assert_eq!(calibration.apply(stuck), [0; 6], "{stuck:?} {calibration:?}");
        }
        // The cap still reaches full speed, and a push past the stuck range still moves.
        assert!(calibration.apply(PRO[0])[0] >= FULL_SCALE - 1, "{calibration:?}");
        assert!(calibration.apply([120, 0, 0, 0, 0, 0])[0] > 0);
    }

    #[test]
    fn a_steady_hold_that_repeats_one_reading_is_still_a_hold() {
        // spacenavd sends only on change, so a cap held still at full deflection repeats one
        // reading for far longer than SETTLE_SECONDS; it is held, not let go.
        let mut wizard = Wizard::default();
        hold(&mut wizard, STUCK[0], REST_SECONDS + 0.1);
        hold(&mut wizard, PRO[0], 1.0);
        assert_eq!((wizard.stage(), wizard.progress()), (Stage::Step(0), 1.0));
        hold(&mut wizard, STUCK[1], SETTLE_SECONDS + 0.1);
        assert_eq!(wizard.stage(), Stage::Step(1));
    }

    /// `reading` and `reading + jitter` in turn, for `seconds`, a frame every 20 ms.
    fn jitter(wizard: &mut Wizard, reading: [i16; 6], jitter: [i16; 6], seconds: f64) {
        let shaken: [i16; 6] = std::array::from_fn(|r| reading[r] + jitter[r]);
        for frame in 0..(seconds / 0.02).round() as usize {
            wizard.feed(if frame % 2 == 0 { reading } else { shaken }, 0.02);
        }
    }

    #[test]
    fn a_drifting_device_that_settles_off_centre_after_each_release_finishes_and_doesnt_creep() {
        // It keeps reporting, noisily, but after each release settles somewhere off its rest:
        // a worn stick, not only spacenavd. The noise stays within its rest wander (`STILL`).
        const NOISE: [i16; 6] = [3, -2, 0, 1, 2, 0];
        let mut wizard = Wizard::default();
        rest_then(&mut wizard);
        for (index, reading) in PRO.into_iter().enumerate() {
            assert_eq!(wizard.stage(), Stage::Step(index));
            if index > 0 {
                // Sitting where the last step settled isn't a hold.
                jitter(&mut wizard, STUCK[index], NOISE, 1.0);
                assert_eq!((wizard.stage(), wizard.progress()), (Stage::Step(index), 0.0));
            }
            hold(&mut wizard, reading, 0.5);
            jitter(&mut wizard, STUCK[index + 1], NOISE, SETTLE_SECONDS + 0.1);
        }
        assert_eq!(wizard.stage(), Stage::Done);
        let calibration = wizard.result().unwrap().unwrap();
        assert!(calibration.axes.iter().enumerate().all(|(c, axis)| axis.source == c && !axis.reversed), "{calibration:?}");
        for stuck in &STUCK[1..] {
            let shaken: [i16; 6] = std::array::from_fn(|r| stuck[r] + NOISE[r]);
            assert_eq!((calibration.apply(*stuck), calibration.apply(shaken)), ([0; 6], [0; 6]), "{stuck:?} {calibration:?}");
        }
        assert!(calibration.apply(PRO[0])[0] >= FULL_SCALE - 1, "{calibration:?}");
    }

    #[test]
    fn both_ends_of_an_odd_width_settled_run_are_untouched_and_one_unit_beyond_is_movement() {
        // Silent at rest (no wander) and a 250 push on X: tolerance is max(4, 250 / 50) = 5, odd.
        // The release dithers over 10..15, as wide as that, so its middle rounds to 13 and 10 is
        // 3 from it, more than half of 5.
        let wizard = || {
            let mut wizard = Wizard::default();
            hold(&mut wizard, [0; 6], REST_SECONDS + 0.1);
            hold(&mut wizard, [250, 0, 0, 0, 0, 0], 0.5);
            cycle(&mut wizard, &[[10, 0, 0, 0, 0, 0], [15, 0, 0, 0, 0, 0]], SETTLE_SECONDS + 0.1);
            assert_eq!(wizard.stage(), Stage::Step(1));
            wizard
        };
        for (x, touched) in [(10, false), (15, false), (9, true), (16, true)] {
            let mut wizard = wizard();
            assert_eq!(wizard.tolerance(0), 5.0);
            wizard.feed([x, 0, 0, 0, 0, 0], 0.02);
            assert_eq!(wizard.progress() > 0.0, touched, "{x}");
        }
    }

    #[test]
    fn a_reading_still_moving_close_to_rest_doesnt_end_a_step() {
        // Past the peak and close to rest, but moving more than the noise: still on the way back.
        let mut wizard = Wizard::default();
        rest_then(&mut wizard);
        hold(&mut wizard, PRO[0], 0.5);
        jitter(&mut wizard, PRO[0].map(|value| value / 5), [20, 0, 0, 0, 0, 0], 1.0);
        assert_eq!(wizard.stage(), Stage::Step(0));
    }

    /// `readings` in turn, from the first, for `seconds`, a frame every 20 ms.
    fn cycle(wizard: &mut Wizard, readings: &[[i16; 6]], seconds: f64) {
        for frame in 0..(seconds / 0.02).round() as usize {
            wizard.feed(readings[frame % readings.len()], 0.02);
        }
    }

    #[test]
    fn an_axis_flat_at_rest_that_dithers_once_off_centre_still_settles() {
        // Silent at rest, so no wander was measured, but ±2 around where each release stops.
        let mut wizard = Wizard::default();
        hold(&mut wizard, [0; 6], REST_SECONDS + 0.1);
        let dither = |at: [i16; 6]| [at.map(|value| value - 2), at, at.map(|value| value + 2)];
        for (index, reading) in PRO.into_iter().enumerate() {
            assert_eq!(wizard.stage(), Stage::Step(index));
            if index > 0 {
                // Dithering where the last step settled doesn't fill this one.
                cycle(&mut wizard, &dither(STUCK[index]), 1.0);
                assert_eq!((wizard.stage(), wizard.progress()), (Stage::Step(index), 0.0));
            }
            hold(&mut wizard, reading, 0.5);
            cycle(&mut wizard, &dither(STUCK[index + 1]), SETTLE_SECONDS + 0.1);
        }
        assert_eq!(wizard.stage(), Stage::Done);
        let calibration = wizard.result().unwrap().unwrap();
        for at in &STUCK[1..] {
            for still in dither(*at) {
                assert_eq!(calibration.apply(still), [0; 6], "{still:?} {calibration:?}");
            }
        }
    }

    #[test]
    fn noise_that_starts_at_one_edge_of_its_spread_is_steady_and_settles_at_its_middle() {
        // X wanders -3..+3 at rest; after the release it cycles -3, 0, +3 around +40.
        let mut wizard = Wizard::default();
        rest_then(&mut wizard);
        hold(&mut wizard, PRO[0], 0.5);
        cycle(&mut wizard, &[[37, 0, 0, 0, 0, 0], [40, 0, 0, 0, 0, 0], [43, 0, 0, 0, 0, 0]], SETTLE_SECONDS + 0.1);
        assert_eq!(wizard.stage(), Stage::Step(1));
        assert_eq!(wizard.recording.settled[0].mean().map(|mean| mean[0].round()), Some(40.0));
    }

    #[test]
    fn the_four_unit_floor_alone_lets_a_two_unit_dither_settle_on_a_short_range() {
        // Flat at rest, and the biggest push only reaches 100, so the reach share is 2 and only
        // the floor (4, i.e. ±2) lets this ±2 dither count as steady.
        let mut wizard = Wizard::default();
        hold(&mut wizard, [0; 6], REST_SECONDS + 0.1);
        hold(&mut wizard, [100, 0, 0, 0, 0, 0], 0.5);
        cycle(&mut wizard, &[[8, 0, 0, 0, 0, 0], [10, 0, 0, 0, 0, 0], [12, 0, 0, 0, 0, 0]], SETTLE_SECONDS + 0.1);
        assert_eq!(wizard.stage(), Stage::Step(1));
    }

    #[test]
    fn dither_the_steadiness_check_rejects_isnt_skipped_as_untouched_either() {
        // Step 0 settles on a ±2 dither around 10. Sitting there is untouched; ±3 around it is wider
        // than the steadiness check calls one position, so it is movement and starts a hold.
        let mut wizard = Wizard::default();
        hold(&mut wizard, [0; 6], REST_SECONDS + 0.1);
        hold(&mut wizard, [100, 0, 0, 0, 0, 0], 0.5);
        cycle(&mut wizard, &[[8, 0, 0, 0, 0, 0], [10, 0, 0, 0, 0, 0], [12, 0, 0, 0, 0, 0]], SETTLE_SECONDS + 0.1);
        assert_eq!(wizard.stage(), Stage::Step(1));
        cycle(&mut wizard, &[[8, 0, 0, 0, 0, 0], [10, 0, 0, 0, 0, 0], [12, 0, 0, 0, 0, 0]], 0.2);
        assert_eq!(wizard.progress(), 0.0);
        cycle(&mut wizard, &[[7, 0, 0, 0, 0, 0], [10, 0, 0, 0, 0, 0], [13, 0, 0, 0, 0, 0]], 0.2);
        assert!(wizard.progress() > 0.0);
    }

    #[test]
    fn a_push_after_a_single_reading_settle_starts_a_hold() {
        // One reading back at rest, not a steady run, is all the settle stores; a full push after
        // it is still movement.
        let mut wizard = Wizard::default();
        hold(&mut wizard, [0; 6], REST_SECONDS + 0.1);
        hold(&mut wizard, [100, 0, 0, 0, 0, 0], 0.5);
        wizard.feed([0; 6], 0.02);
        assert_eq!(wizard.stage(), Stage::Step(1));
        assert_eq!(wizard.settled.map(|settled| settled.count), Some(1));
        hold(&mut wizard, [-100, 0, 0, 0, 0, 0], 0.2);
        assert_eq!((wizard.stage(), wizard.progress() > 0.0), (Stage::Step(1), true));
    }

    #[test]
    fn a_narrow_settled_run_skips_a_whole_tolerance_either_side_and_the_band_stays_anchored() {
        // Flat at rest and a 100 push: tolerance is exactly 4. The release settles flat at 10, so a
        // reading 4 either side (6, 14) is untouched, repeated or not; one more unit is movement.
        let wizard = || {
            let mut wizard = Wizard::default();
            hold(&mut wizard, [0; 6], REST_SECONDS + 0.1);
            hold(&mut wizard, [100, 0, 0, 0, 0, 0], 0.5);
            hold(&mut wizard, [10, 0, 0, 0, 0, 0], SETTLE_SECONDS + 0.1);
            assert_eq!(wizard.stage(), Stage::Step(1));
            assert_eq!(wizard.tolerance(0), 4.0);
            wizard
        };
        let mut edges = wizard();
        cycle(&mut edges, &[[6, 0, 0, 0, 0, 0], [14, 0, 0, 0, 0, 0]], 0.2);
        hold(&mut edges, [14, 0, 0, 0, 0, 0], 0.2);
        assert_eq!(edges.progress(), 0.0);
        // Skipped readings don't widen the band: after all those 14s, 15 is still movement.
        edges.feed([15, 0, 0, 0, 0, 0], 0.02);
        assert!(edges.progress() > 0.0);
        for x in [5, 15] {
            let mut wizard = wizard();
            wizard.feed([x, 0, 0, 0, 0, 0], 0.02);
            assert!(wizard.progress() > 0.0, "{x}");
        }
    }

    #[test]
    fn a_step_settles_on_its_own_axis_when_another_with_a_wider_range_stays_out() {
        // The step's own axis only goes to 100; another axis stays 30 out after the release,
        // more than a quarter of 100, which mustn't hold the step up.
        let mut wizard = Wizard::default();
        hold(&mut wizard, [0; 6], REST_SECONDS + 0.1);
        hold(&mut wizard, [100, 0, 0, 0, 0, 0], 0.5);
        hold(&mut wizard, [5, 30, 0, 0, 0, 0], SETTLE_SECONDS + 0.1);
        assert_eq!(wizard.stage(), Stage::Step(1));
    }

    #[test]
    fn a_step_stays_held_when_the_lesser_axis_is_held_while_the_larger_one_relaxes() {
        for held in [[100, 5, 0, 0, 0, 0], [100, 60, 0, 0, 0, 0], [100, 200, 0, 0, 0, 0]] {
            let mut wizard = Wizard::default();
            hold(&mut wizard, [0; 6], REST_SECONDS + 0.1);
            hold(&mut wizard, [100, 200, 0, 0, 0, 0], 0.5);
            hold(&mut wizard, held, SETTLE_SECONDS + 0.1);
            assert_eq!(wizard.stage(), Stage::Step(0), "{held:?}");
        }
        // A plain hold of a single axis.
        let mut wizard = Wizard::default();
        hold(&mut wizard, [0; 6], REST_SECONDS + 0.1);
        hold(&mut wizard, [100, 0, 0, 0, 0, 0], 0.5);
        hold(&mut wizard, [100, 0, 0, 0, 0, 0], SETTLE_SECONDS + 0.1);
        assert_eq!(wizard.stage(), Stage::Step(0));
    }

    #[test]
    fn a_step_settles_when_cross_talk_moved_another_axis_further_than_its_own() {
        // Nula's case: the push moves raw 1 twice as far as raw 0, which may be the one asked
        // for. Raw 0 comes back to 5 % and raw 1 stays at 30 %: let go, both steady.
        let mut wizard = Wizard::default();
        hold(&mut wizard, [0; 6], REST_SECONDS + 0.1);
        hold(&mut wizard, [100, 200, 0, 0, 0, 0], 0.5);
        hold(&mut wizard, [5, 60, 0, 0, 0, 0], SETTLE_SECONDS + 0.1);
        assert_eq!(wizard.stage(), Stage::Step(1));
        // Still holding the push, with only the lesser axis back, isn't let go.
        let mut holding = Wizard::default();
        hold(&mut holding, [0; 6], REST_SECONDS + 0.1);
        hold(&mut holding, [100, 200, 0, 0, 0, 0], 0.5);
        hold(&mut holding, [5, 200, 0, 0, 0, 0], 1.0);
        assert_eq!(holding.stage(), Stage::Step(0));
    }

    #[test]
    fn a_lopsided_dither_that_settled_a_step_isnt_movement_in_the_next() {
        // Mostly 10 with an occasional 14: spread 4, so steady, but the mean sits near 11 and the 14
        // is the far edge. Sitting there stays untouched; 15 is wider than the steadiness check allows.
        let mut wizard = Wizard::default();
        hold(&mut wizard, [0; 6], REST_SECONDS + 0.1);
        hold(&mut wizard, [100, 0, 0, 0, 0, 0], 0.5);
        let lopsided = [[10, 0, 0, 0, 0, 0], [10, 0, 0, 0, 0, 0], [10, 0, 0, 0, 0, 0], [14, 0, 0, 0, 0, 0]];
        cycle(&mut wizard, &lopsided, SETTLE_SECONDS + 0.1);
        assert_eq!(wizard.stage(), Stage::Step(1));
        cycle(&mut wizard, &lopsided, 1.0);
        assert_eq!((wizard.stage(), wizard.progress()), (Stage::Step(1), 0.0));
        hold(&mut wizard, [16, 0, 0, 0, 0, 0], 0.2);
        assert!(wizard.progress() > 0.0);
    }

    #[test]
    fn cross_talk_that_leaves_the_bigger_axis_well_out_keeps_the_step_open() {
        // The documented limit (Grok's case): raw 0 moved less than half as far as raw 1, so it
        // never qualifies, and raw 1 at 90 is above a quarter of its 300 peak.
        let mut wizard = Wizard::default();
        hold(&mut wizard, [0; 6], REST_SECONDS + 0.1);
        hold(&mut wizard, [100, 300, 0, 0, 0, 0], 0.5);
        hold(&mut wizard, [5, 90, 0, 0, 0, 0], SETTLE_SECONDS + 1.0);
        assert_eq!(wizard.stage(), Stage::Step(0));
    }

    #[test]
    fn a_hand_held_still_close_to_rest_after_the_push_counts_as_let_go() {
        // The known limit: held exactly still at a fifth of the way out, on any device, ends the
        // step there, and that axis's dead zone widens to cover it.
        let low = PRO[0].map(|value| value / 5);
        let mut wizard = Wizard::default();
        rest_then(&mut wizard);
        hold(&mut wizard, PRO[0], 0.5);
        hold(&mut wizard, low, SETTLE_SECONDS + 0.1);
        assert_eq!(wizard.stage(), Stage::Step(1));
        assert_eq!((wizard.recording.settled[0].min, wizard.recording.settled[0].max), (low, low));
        // Coming back to its true rest afterwards doesn't undo that: the next step is waiting.
        hold(&mut wizard, STILL[0], 0.5);
        assert_eq!((wizard.stage(), wizard.progress()), (Stage::Step(1), 0.0));
        // No frame of where it settled went into the step as held.
        assert_eq!((wizard.recording.steps[0].min[0], wizard.recording.steps[0].max[4]), (PRO[0][0], PRO[0][4]));
    }

    #[test]
    fn every_reading_of_a_release_that_was_let_go_falls_in_the_dead_zone() {
        // Nula's case: X wanders -4..+4 at rest. Half the releases dither over +8..+16, the other
        // half over -16..-8. Keeping one reading of each, padded by the wander, can miss the edges.
        let wander = [[-4, 0, 0, 0, 0, 0], [4, 0, 0, 0, 0, 0]];
        let x = |value: i16| [value, 0, 0, 0, 0, 0];
        let (above, below) = ([x(16), x(12), x(8)], [x(-8), x(-12), x(-16)]);
        let mut wizard = Wizard::default();
        for _ in 0..40 {
            hold(&mut wizard, wander[0], 0.02);
            hold(&mut wizard, wander[1], 0.02);
        }
        for (index, reading) in PRO.into_iter().enumerate() {
            assert_eq!(wizard.stage(), Stage::Step(index));
            hold(&mut wizard, reading, 0.5);
            cycle(&mut wizard, if index % 2 == 0 { &above } else { &below }, SETTLE_SECONDS + 0.1);
        }
        let calibration = wizard.result().unwrap().unwrap();
        for still in above.iter().chain(&below).chain(&wander) {
            assert_eq!(calibration.apply(*still), [0; 6], "{still:?} {calibration:?}");
        }
    }

    #[test]
    fn two_steps_in_a_row_that_stop_at_the_same_place_both_finish() {
        let mut wizard = Wizard::default();
        hold(&mut wizard, STUCK[0], REST_SECONDS + 0.1);
        for reading in &PRO[..2] {
            hold(&mut wizard, *reading, 0.5);
            hold(&mut wizard, STUCK[1], SETTLE_SECONDS + 0.1);
        }
        assert_eq!(wizard.stage(), Stage::Step(2));
    }

    #[test]
    fn recentring_on_where_the_cap_settled_keeps_the_whole_rest_wander_inside_the_dead_zone() {
        // X wanders -4..+4 at rest, and every release settles at +12.
        let wander = [[-4, 0, 0, 0, 0, 0], [4, 0, 0, 0, 0, 0]];
        let mut recording = record(PRO, wander);
        for index in 0..12 {
            recording.settled(index, [12, 0, 0, 0, 0, 0]);
        }
        let calibration = recording.solve().unwrap();
        for still in [wander[0], wander[1], [12, 0, 0, 0, 0, 0]] {
            assert_eq!(calibration.apply(still), [0; 6], "{still:?} {calibration:?}");
        }
    }

    #[test]
    fn the_dead_zone_shares_are_where_apply_starts_moving() {
        // A lopsided, reversed axis with the user's dead zone on top.
        let mut calibration = walk(STILL, PRO).result().unwrap().unwrap();
        calibration.axes[0] = AxisCalibration { source: 0, reversed: true, rest: 10, dead_zone: 20, full: [100, 400] };
        calibration.dead_zone_share = 0.1;
        let [negative, positive] = calibration.dead_zone_shares()[0];
        // Canonical negative is raw above rest here: 400 of travel, 20 measured.
        assert!((negative - (0.05 + 0.95 * 0.1)).abs() < 1e-6 && (positive - (0.2 + 0.8 * 0.1)).abs() < 1e-6);
        let at = |raw: i16| [raw, 0, 0, 0, 0, 0];
        let inside = |share: f32, full: f32| (share * full).floor() as i16 - 1;
        assert_eq!(calibration.apply(at(10 + inside(negative, 400.0)))[0], 0);
        assert!(calibration.apply(at(10 + inside(negative, 400.0) + 3))[0] < 0);
        assert_eq!(calibration.apply(at(10 - inside(positive, 100.0)))[0], 0);
        assert!(calibration.apply(at(10 - inside(positive, 100.0) - 3))[0] > 0);
        // Full deflection is ±1 of travel, the reversed way round.
        assert_eq!(calibration.travel(at(410))[0], -1.0);
        assert_eq!(calibration.travel(at(-90))[0], 1.0);
    }

    #[test]
    fn a_device_that_comes_back_to_rest_keeps_its_measured_rest_and_dead_zone() {
        let mut settled = record(PRO, STILL);
        let plain = settled.solve().unwrap();
        for index in 0..12 {
            settled.settled(index, STILL[index % 2]);
        }
        assert_eq!(settled.solve().unwrap(), plain);
    }

    #[test]
    fn a_noisy_axis_at_rest_holds_up_neither_itself_nor_a_quiet_one() {
        // X wanders ±4 at rest, and goes +500 one way but only -50 the other: the solver accepts
        // that, so the wizard has to be able to record it.
        let mut steps = PRO;
        steps[0][0] = 500;
        steps[1][0] = -50;
        let noisy_x = [[-4, 0, 0, 0, 0, 0], [4, 0, 0, 0, 0, 0]];
        let calibration = walk(noisy_x, steps).result().unwrap().unwrap();
        assert_eq!((calibration.axes[0].source, calibration.axes[0].dead_zone, calibration.axes[0].full), (0, 4, [50, 500]));
        // Rz wanders ±40 at rest: every other axis still records against its own quiet rest.
        let noisy_rz = [[0, 0, 0, 0, 0, -40], [0, 0, 0, 0, 0, 40]];
        let calibration = walk(noisy_rz, PRO).result().unwrap().unwrap();
        assert!(calibration.axes.iter().enumerate().all(|(c, axis)| axis.source == c && !axis.reversed), "{calibration:?}");
    }

    #[test]
    fn a_step_that_caught_a_resting_hand_instead_of_a_push_is_named() {
        // A quiet device (nothing at rest), and slide right recorded only a hand resting on the
        // cap: scaled up, a few units would be full speed.
        let mut steps = PRO;
        steps[0] = [6, 4, 0, 0, 3, 0];
        assert_eq!(record(steps, [[0; 6]; 2]).solve(), Err(Problem::NoResponse(0)));
    }

    #[test]
    fn a_hand_edited_calibration_that_could_spin_the_view_isnt_usable() {
        let solved = record(PRO, STILL).solve().unwrap();
        assert!(solved.is_usable());
        let mut bad = solved;
        bad.axes[2].source = 6;
        assert!(!bad.is_usable(), "a source the device doesn't have");
        let mut bad = solved;
        bad.axes[2].full[0] = bad.axes[2].dead_zone;
        assert!(!bad.is_usable(), "any touch past the dead zone would be full speed");
        let mut bad = solved;
        bad.axes[2].source = bad.axes[3].source;
        assert!(!bad.is_usable(), "two canonical axes from one raw axis");
    }

    #[test]
    fn readings_for_a_step_that_doesnt_exist_are_ignored() {
        let mut recording = record(PRO, STILL);
        recording.step(12, [i16::MAX; 6]);
        assert_eq!(recording.solve(), record(PRO, STILL).solve());
    }
}
