//! Driving dynamics.
//!
//! A transient four wheel vehicle model with a Magic Formula tyre, run through the manoeuvres a
//! chassis engineer would ask for. This is the first thing in the suite that says how a vehicle
//! behaves rather than what it is made of.
//!
//! What it is not: it has no suspension kinematics, so camber and toe do not change as the
//! wheels move, and no springs, because the library has none yet. Both of those matter, and
//! until they exist the roll stiffness is an assumption rather than a result. Every report says
//! which of its inputs were assumed, in the report itself, because a number with a hidden
//! assumption behind it is worse than no number.

mod manoeuvre;
mod report;
mod solver;
mod tyre;
mod vehicle;

pub use manoeuvre::{
    Acceleration, Braking, LaneChange, SteadyState, StepSteer, acceleration, braking, lane_change,
    steady_state, step_steer,
};
pub use report::write_text;
pub use solver::{Controls, Outputs, State, derivatives, step};
pub use tyre::Tyre;
pub use vehicle::{Input, SimVehicle, Source};

/// Everything the standard set of manoeuvres found.
pub struct Handling {
    pub vehicle: SimVehicle,
    pub skidpad: SteadyState,
    pub step: StepSteer,
    pub braking: Braking,
    pub acceleration: Acceleration,
    pub lane_change: LaneChange,
}

impl Handling {
    /// Run the standard set.
    pub fn run(veh: SimVehicle) -> Handling {
        let skidpad = steady_state(&veh, 30.0);
        let step = step_steer(&veh, 80.0 / 3.6, 2.0);
        let braking = braking(&veh, 100.0);
        let acceleration = acceleration(&veh);
        let lane_change = lane_change(&veh, 70.0);
        Handling {
            vehicle: veh,
            skidpad,
            step,
            braking,
            acceleration,
            lane_change,
        }
    }

    /// How the vehicle is balanced, in words.
    pub fn balance(&self) -> &'static str {
        let k = self.skidpad.understeer_gradient;
        if k > 4.0 {
            "strong understeer: it will run wide and resist turning in"
        } else if k > 1.0 {
            "understeer, which is the stable and legal default for a road car"
        } else if k > -0.5 {
            "close to neutral: quick, and less forgiving at the limit"
        } else {
            "OVERSTEER: the rear lets go first, which is unsafe on a road car"
        }
    }
}
