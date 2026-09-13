//! Running the vehicle into the barrier.
//!
//! Explicit integration of a chain of masses joined by crushable elements. Small time step, no
//! matrix, no iteration: the same scheme a real explicit code uses, on a model small enough to
//! run in a millisecond.

use crate::model::CrashModel;
use crate::pulse::{Pulse, occupant_load_criterion};
use crate::Standard;

#[derive(Debug, Clone)]
pub struct Impact {
    pub standard: Standard,
    pub speed: f64,
}

#[derive(Debug, Clone)]
pub struct CrashResult {
    pub standard: Standard,
    pub speed_kph: f64,
    pub mass: f64,
    /// Kinetic energy going in, joules.
    pub energy: f64,
    /// What the structure ahead of the compartment could absorb, joules.
    pub capacity: f64,
    /// Total crush, metres.
    pub crush: f64,
    /// How far the front of the occupant compartment moved, metres. This is the number that
    /// decides whether anybody walks away.
    pub intrusion: f64,
    /// True when the crush zone ran out and the compartment began to take load.
    pub bottomed_out: bool,
    /// Deceleration of the occupant compartment over time.
    pub pulse: Pulse,
    pub peak_g: f64,
    pub mean_g: f64,
    /// Occupant load criterion, in g. The measure used to judge a pulse.
    pub olc: f64,
    pub duration: f64,
    /// True when every material in the crush structure has been validated by test.
    pub validated: bool,
    pub notes: Vec<String>,
}

/// Run one impact.
///
/// Displacement is positive forward, toward the barrier. Every slice starts at zero moving at
/// the impact speed. Element `j` joins slice `j` in front to slice `j + 1` behind, and is in
/// compression by however much the one behind has caught up with the one in front.
pub fn run(model: &CrashModel, standard: Standard) -> CrashResult {
    let speed = standard.speed_kph() / 3.6;
    let mut notes = model.notes.clone();

    // Where the occupants start. Everything from there rearward is one rigid body: a
    // compartment that crushes is a failed design, not a thing to model the deformation of, and
    // lumping it also keeps the chain short enough to integrate cleanly.
    let compartment = model
        .slices
        .iter()
        .position(|s| s.occupied)
        .unwrap_or(model.slices.len().saturating_sub(1));

    let zone: Vec<f64> = model
        .slices
        .iter()
        .take(compartment)
        .map(|s| s.mass)
        .collect();
    let rear: f64 = model.slices.iter().skip(compartment).map(|s| s.mass).sum();
    let elements: Vec<&crate::model::Element> =
        model.elements.iter().take(compartment).collect();

    if elements.is_empty() || rear <= 0.0 {
        notes.push(
            "There is no crushable structure between the barrier and the occupants."
                .into(),
        );
        return empty(standard, speed, model.mass, notes);
    }

    // The chain: the crush zone slices, then the compartment and everything behind it.
    let mut mass: Vec<f64> = zone.iter().map(|m| m.max(1.0)).collect();
    mass.push(rear);
    let n = mass.len();
    let total: f64 = mass.iter().sum();

    let mut x = vec![0.0_f64; n];
    let mut v = vec![speed; n];

    let capacity: f64 = elements.iter().map(|e| e.capacity).sum();
    let energy = 0.5 * total * speed * speed;
    let validated = elements.iter().all(|e| e.validated);

    let dt = 1.0e-6;
    let mut t = 0.0;
    let mut history: Vec<(f64, f64)> = Vec::new();
    let mut peak_g: f64 = 0.0;
    let body = n - 1;

    while t < 0.4 {
        let mut f = vec![0.0_f64; n - 1];
        for j in 0..n - 1 {
            let squash = (x[j + 1] - x[j]).max(0.0);
            f[j] = element_force(elements[j.min(elements.len() - 1)], squash);
        }

        let mut a = vec![0.0_f64; n];
        for j in 0..n {
            let forward = if j < f.len() { f[j] } else { 0.0 };
            let back = if j > 0 { f[j - 1] } else { 0.0 };
            a[j] = (forward - back) / mass[j];
        }
        // A rigid barrier pushes, never pulls, and the front face cannot go through it.
        if x[0] >= 0.0 && v[0] >= 0.0 {
            a[0] = a[0].min(0.0);
        }

        for j in 0..n {
            v[j] += a[j] * dt;
            x[j] += v[j] * dt;
        }
        if x[0] > 0.0 {
            x[0] = 0.0;
            v[0] = v[0].min(0.0);
        }

        let g = -a[body] / 9.81;
        peak_g = peak_g.max(g);
        history.push((t, g));
        t += dt;

        if v[body] <= 0.0 {
            break;
        }
    }

    // How far the compartment travelled into the structure in front of it.
    let crush = (x[body] - x[0]).max(0.0);
    // Whether the crush zone ran out. Measured on the whole zone rather than any one slice,
    // because the front of it going solid is normal: that is how progressive crush works.
    let available: f64 = elements.iter().map(|e| e.stroke).sum();
    let bottomed = crush >= available * 0.98;
    let intrusion = if bottomed { crush - available } else { 0.0 };

    let duration = t;
    let pulse = Pulse::from_history(&history, speed);
    let olc = occupant_load_criterion(&pulse, speed);
    let mean_g = if duration > 1e-6 {
        (speed - v[body].max(0.0)) / duration / 9.81
    } else {
        0.0
    };

    if capacity < energy {
        notes.push(format!(
            "The structure ahead of the occupants can absorb {:.0} kJ and the impact brings {:.0} kJ.",
            capacity / 1e3,
            energy / 1e3
        ));
    }
    if !validated {
        notes.push(
            "At least one material in the crush structure has not had its crush behaviour measured on a coupon, so this result is not evidence of anything."
                .into(),
        );
    }
    if duration >= 0.39 {
        notes.push(
            "The compartment never came to rest within 400 ms, so the model did not resolve this impact. Treat the numbers above as unreliable."
                .into(),
        );
    }

    CrashResult {
        standard,
        speed_kph: standard.speed_kph(),
        mass: total,
        energy,
        capacity,
        crush,
        intrusion,
        bottomed_out: bottomed,
        pulse,
        peak_g,
        mean_g,
        olc,
        duration,
        validated,
        notes,
    }
}

fn empty(standard: Standard, speed: f64, mass: f64, notes: Vec<String>) -> CrashResult {
    CrashResult {
        standard,
        speed_kph: standard.speed_kph(),
        mass,
        energy: 0.5 * mass * speed * speed,
        capacity: 0.0,
        crush: 0.0,
        intrusion: 0.0,
        bottomed_out: true,
        pulse: Pulse::default(),
        peak_g: 0.0,
        mean_g: 0.0,
        olc: 0.0,
        duration: 0.0,
        validated: false,
        notes,
    }
}

/// The force a crushable element carries at a given amount of squash.
///
/// Three regions: a rise to the trigger peak over the first few millimetres, a steady plateau
/// which is where nearly all the energy goes, and a steep rise once the debris packs solid.
fn element_force(e: &crate::model::Element, squash: f64) -> f64 {
    if squash <= 0.0 {
        return 0.0;
    }
    let trigger = 0.006_f64;
    if squash < trigger {
        // The initial peak. Blunt, but this is the part of the pulse that hurts.
        let f = squash / trigger;
        return e.peak_force * f;
    }
    if squash < e.stroke {
        // Falling from the peak to the plateau over the first tenth of the stroke.
        let settle = (e.stroke * 0.1).max(1e-4);
        let over = ((squash - trigger) / settle).min(1.0);
        return e.peak_force + (e.force - e.peak_force) * over;
    }
    // Solid. Everything behind it now pushes on everything in front, and the force rises
    // steeply rather than without limit, because the structure still yields.
    // Solid. Everything still yields, so the force rises steeply rather than without bound;
    // an infinitely rigid stop would put a numerical spike in the pulse and nothing else.
    let past = squash - e.stroke;
    e.force * (1.0 + past * 60.0).min(6.0)
}
