//! Crash.
//!
//! What this is: a lumped mass model. The vehicle is cut into slices along its length, each slice
//! carrying the mass that lives there, joined by crushable elements whose force comes from the
//! structure actually present in that slice and the crush behaviour of its material. It is
//! integrated explicitly against a barrier.
//!
//! What this is not: finite element. A real crash result needs an explicit finite element
//! solution with a validated material model, because the things that decide the answer are
//! buckling modes, joint failures and contact, and none of those exist in a chain of springs.
//!
//! So why have it. Because in the phase where a vehicle is still being laid out, the questions
//! are how much crush length there is, whether the structure ahead of the cabin can absorb the
//! energy before it packs solid, and what pulse that gives the occupants. A lumped mass model
//! answers those in milliseconds while the design is still moving, and it answers them from the
//! same model the rest of the suite uses rather than from a spreadsheet.
//!
//! Every result is labelled as screening. The compliance rule pack already refuses to accept an
//! uncalibrated composite crush result as evidence of anything, which is exactly right.

mod model;
mod pulse;
mod report;
mod solve;

pub use model::{CrashModel, Element, Slice, build_model};
pub use pulse::{Pulse, occupant_load_criterion};
pub use report::write_text;
pub use solve::{CrashResult, Impact, run};

/// The standard impacts, and where they come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Standard {
    /// Full width rigid barrier at 56 km/h. ADR 69, the frontal impact rule.
    FullFrontal,
    /// 40 per cent overlap into a deformable barrier at 64 km/h. ADR 73.
    OffsetFrontal,
    /// A pole at 32 km/h, which is the hardest test of a narrow crush zone.
    Pole,
}

impl Standard {
    pub fn speed_kph(&self) -> f64 {
        match self {
            Standard::FullFrontal => 56.0,
            Standard::OffsetFrontal => 64.0,
            Standard::Pole => 32.0,
        }
    }

    /// How much of the structure's width is engaged.
    pub fn overlap(&self) -> f64 {
        match self {
            Standard::FullFrontal => 1.0,
            Standard::OffsetFrontal => 0.4,
            Standard::Pole => 0.25,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Standard::FullFrontal => "full width rigid barrier",
            Standard::OffsetFrontal => "40 per cent offset deformable barrier",
            Standard::Pole => "rigid pole",
        }
    }

    pub fn source(&self) -> &'static str {
        match self {
            Standard::FullFrontal => "ADR 69: 56 km/h into a rigid barrier",
            Standard::OffsetFrontal => "ADR 73: 64 km/h, 40 per cent overlap",
            Standard::Pole => "Not an ADR test; the hardest case for a narrow crush zone",
        }
    }

    pub fn all() -> [Standard; 3] {
        [
            Standard::FullFrontal,
            Standard::OffsetFrontal,
            Standard::Pole,
        ]
    }
}
