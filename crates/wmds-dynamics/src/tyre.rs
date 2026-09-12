//! The tyre model.
//!
//! Everything a vehicle does on the road happens through four contact patches the size of a
//! hand, so a handling result is only ever as good as the tyre model under it. This is a Magic
//! Formula in the usual Pacejka shape, with the two things that matter most for a road car:
//! grip that falls away as vertical load rises, and a friction ellipse so a tyre cannot produce
//! its full cornering force while it is also braking hard.
//!
//! The coefficients come from the tyre primitive's behaviour block, not from here. What is in
//! this file is the maths; what a particular tyre does is data, like everything else in the
//! suite.

/// A tyre's coefficients, already resolved from a primitive.
#[derive(Debug, Clone)]
pub struct Tyre {
    /// Metres, unloaded.
    pub radius: f64,
    /// Metres, the radius that actually decides road speed for a given wheel speed.
    pub rolling_radius: f64,
    /// Newtons. The load the friction figures are quoted at.
    pub nominal_load: f64,
    pub peak_friction_y: f64,
    pub peak_friction_x: f64,
    /// How much peak friction is lost when the load doubles.
    pub load_sensitivity: f64,
    pub cornering_c1: f64,
    pub cornering_c2: f64,
    /// Longitudinal slip stiffness as a multiple of vertical load.
    pub slip_stiffness: f64,
    pub shape_y: f64,
    pub shape_x: f64,
    pub curvature_y: f64,
    pub curvature_x: f64,
    /// Metres of rolling before most of a new slip angle has built up.
    pub relaxation_length: f64,
    pub rolling_resistance: f64,
}

impl Default for Tyre {
    /// A plain road tyre, for when a vehicle has no tyre in it yet.
    ///
    /// Using this rather than refusing to run is deliberate, but every report that does has to
    /// say so, because a handling number from a guessed tyre is a guess.
    fn default() -> Tyre {
        Tyre {
            radius: 0.3,
            rolling_radius: 0.29,
            nominal_load: 3500.0,
            peak_friction_y: 1.05,
            peak_friction_x: 1.15,
            load_sensitivity: 0.10,
            cornering_c1: 26.0,
            cornering_c2: 2.2,
            slip_stiffness: 18.0,
            // The lateral shape and curvature together decide where the curve peaks. A road
            // tyre peaks somewhere around eight degrees of slip; a curvature near one pushes
            // that out past forty, which is a racing slick at best and usually a mistake.
            shape_y: 1.50,
            shape_x: 1.65,
            curvature_y: 0.0,
            curvature_x: 0.0,
            relaxation_length: 0.45,
            rolling_resistance: 0.011,
        }
    }
}

impl Tyre {
    /// Peak friction available at this vertical load.
    ///
    /// A tyre carrying twice its nominal load does not produce twice the force. That single
    /// effect is why load transfer costs grip, and therefore why a stiff anti-roll bar at one
    /// end changes the balance of the car.
    pub fn mu_y(&self, fz: f64) -> f64 {
        self.peak_friction_y * (1.0 - self.load_sensitivity * (fz / self.nominal_load - 1.0))
    }

    pub fn mu_x(&self, fz: f64) -> f64 {
        self.peak_friction_x * (1.0 - self.load_sensitivity * (fz / self.nominal_load - 1.0))
    }

    /// Cornering stiffness in newtons per radian at this load.
    ///
    /// Rises with load and then flattens off, which is the standard shape and the reason a
    /// lightly loaded inside wheel contributes far less than its share.
    pub fn cornering_stiffness(&self, fz: f64) -> f64 {
        if fz <= 0.0 {
            return 0.0;
        }
        self.cornering_c1
            * self.nominal_load
            * (2.0 * (fz / (self.cornering_c2 * self.nominal_load)).atan()).sin()
    }

    /// Lateral force for a slip angle in radians, at a vertical load in newtons.
    ///
    /// Sign convention: a positive slip angle produces a negative lateral force, the usual one,
    /// so a tyre pushed to the left resists to the right.
    pub fn lateral(&self, slip: f64, fz: f64) -> f64 {
        if fz <= 0.0 {
            return 0.0;
        }
        let d = self.mu_y(fz).max(0.0) * fz;
        let c = self.shape_y;
        let e = self.curvature_y;
        let k = self.cornering_stiffness(fz);
        if d <= 0.0 || k <= 0.0 {
            return 0.0;
        }
        let b = k / (c * d);
        let bx = b * slip;
        -d * (c * (bx - e * (bx - bx.atan())).atan()).sin()
    }

    /// Longitudinal force for a slip ratio, at a vertical load in newtons.
    ///
    /// Positive slip ratio means the tyre is driving; negative means braking.
    pub fn longitudinal(&self, slip_ratio: f64, fz: f64) -> f64 {
        if fz <= 0.0 {
            return 0.0;
        }
        let d = self.mu_x(fz).max(0.0) * fz;
        let c = self.shape_x;
        let e = self.curvature_x;
        let k = self.slip_stiffness * fz;
        if d <= 0.0 || k <= 0.0 {
            return 0.0;
        }
        let b = k / (c * d);
        let bx = b * slip_ratio;
        d * (c * (bx - e * (bx - bx.atan())).atan()).sin()
    }

    /// Both forces at once, scaled so the pair cannot exceed what the tyre has.
    ///
    /// The friction ellipse. Without it a model will happily brake at the limit and corner at
    /// the limit simultaneously, which is the single most common way a vehicle simulation
    /// flatters a design.
    pub fn combined(&self, slip: f64, slip_ratio: f64, fz: f64) -> (f64, f64) {
        let fx = self.longitudinal(slip_ratio, fz);
        let fy = self.lateral(slip, fz);
        let max_x = self.mu_x(fz).max(0.0) * fz;
        let max_y = self.mu_y(fz).max(0.0) * fz;
        if max_x <= 0.0 || max_y <= 0.0 {
            return (0.0, 0.0);
        }
        let demand = ((fx / max_x).powi(2) + (fy / max_y).powi(2)).sqrt();
        if demand <= 1.0 {
            (fx, fy)
        } else {
            (fx / demand, fy / demand)
        }
    }

    /// The slip angle at which the tyre makes its most lateral force, in radians.
    pub fn peak_slip_angle(&self, fz: f64) -> f64 {
        let mut best = 0.0;
        let mut best_f = 0.0;
        let mut a = 0.0;
        while a < 0.35 {
            let f = -self.lateral(a, fz);
            if f > best_f {
                best_f = f;
                best = a;
            }
            a += 0.0005;
        }
        best
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t() -> Tyre {
        Tyre::default()
    }

    #[test]
    fn lateral_force_rises_then_falls_away() {
        let ty = t();
        let fz = 3500.0;
        let small = -ty.lateral(0.02, fz);
        let peak_angle = ty.peak_slip_angle(fz);
        let peak = -ty.lateral(peak_angle, fz);
        let beyond = -ty.lateral(0.30, fz);
        assert!(small > 0.0);
        assert!(peak > small, "force should still be climbing at 1 degree");
        assert!(
            beyond < peak,
            "a tyre past its peak makes less grip, not more: {beyond:.0} N against {peak:.0} N"
        );
        // A road tyre peaks somewhere between about four and twelve degrees.
        let deg = peak_angle.to_degrees();
        assert!(
            (4.0..12.0).contains(&deg),
            "peak slip angle of {deg:.1} degrees is not a road tyre"
        );
    }

    #[test]
    fn peak_force_is_near_the_friction_limit() {
        let ty = t();
        let fz = 3500.0;
        let peak = -ty.lateral(ty.peak_slip_angle(fz), fz);
        let limit = ty.mu_y(fz) * fz;
        let ratio = peak / limit;
        assert!(
            (0.95..=1.001).contains(&ratio),
            "the peak should reach the stated friction, got {ratio:.3} of it"
        );
    }

    #[test]
    fn grip_per_newton_falls_as_load_rises() {
        // The whole reason load transfer costs a car grip.
        let ty = t();
        let light = {
            let fz = 2000.0;
            -ty.lateral(ty.peak_slip_angle(fz), fz) / fz
        };
        let heavy = {
            let fz = 6000.0;
            -ty.lateral(ty.peak_slip_angle(fz), fz) / fz
        };
        assert!(
            heavy < light,
            "a heavily loaded tyre must give less grip per newton: {heavy:.3} against {light:.3}"
        );
    }

    #[test]
    fn a_tyre_cannot_brake_and_corner_at_full_capability_at_once() {
        let ty = t();
        let fz = 3500.0;
        let pure_y = -ty.lateral(ty.peak_slip_angle(fz), fz);
        let (fx, fy) = ty.combined(ty.peak_slip_angle(fz), -0.15, fz);
        assert!(
            -fy < pure_y,
            "cornering force should drop while braking hard: {:.0} N against {pure_y:.0} N",
            -fy
        );
        let max_x = ty.mu_x(fz) * fz;
        let max_y = ty.mu_y(fz) * fz;
        let demand = ((fx / max_x).powi(2) + (fy / max_y).powi(2)).sqrt();
        assert!(demand <= 1.0001, "combined demand of {demand:.3} exceeds the tyre");
    }

    #[test]
    fn an_unloaded_tyre_makes_no_force() {
        // An inside wheel lifting off must contribute nothing, not a negative number.
        let ty = t();
        assert_eq!(ty.lateral(0.1, 0.0), 0.0);
        assert_eq!(ty.longitudinal(0.1, 0.0), 0.0);
        assert_eq!(ty.combined(0.1, 0.1, -50.0), (0.0, 0.0));
    }
}
