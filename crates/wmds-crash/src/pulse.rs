//! The deceleration pulse, and how it is judged.
//!
//! Peak deceleration on its own says very little about injury. What matters is how quickly the
//! occupant, who is not yet restrained, is brought up to the speed of the car around them. The
//! occupant load criterion is the standard way of putting a number on that, and it is what a
//! pulse is compared against when a restraint system is specified.

#[derive(Debug, Clone, Default)]
pub struct Pulse {
    /// Time in seconds against deceleration in g.
    pub samples: Vec<(f64, f64)>,
    /// Impact speed the pulse came from, m/s.
    pub speed: f64,
}

impl Pulse {
    pub fn from_history(history: &[(f64, f64)], speed: f64) -> Pulse {
        // Thinned to a manageable number of points; the shape matters, not every step.
        let keep = 400.max(1);
        let stride = (history.len() / keep).max(1);
        Pulse {
            samples: history.iter().step_by(stride).copied().collect(),
            speed,
        }
    }

    pub fn peak(&self) -> f64 {
        self.samples.iter().map(|(_, g)| *g).fold(0.0, f64::max)
    }

    pub fn duration(&self) -> f64 {
        self.samples.last().map(|(t, _)| *t).unwrap_or(0.0)
    }

    /// A coarse picture of the pulse, for a terminal.
    pub fn sparkline(&self, width: usize) -> String {
        if self.samples.is_empty() {
            return String::new();
        }
        let peak = self.peak().max(1e-6);
        let bars = [' ', '.', ':', '-', '=', '+', '*', '#'];
        let mut out = String::with_capacity(width);
        for i in 0..width {
            let a = i as f64 / width as f64;
            let b = (i + 1) as f64 / width as f64;
            let t0 = self.duration() * a;
            let t1 = self.duration() * b;
            let g = self
                .samples
                .iter()
                .filter(|(t, _)| *t >= t0 && *t < t1)
                .map(|(_, g)| *g)
                .fold(0.0, f64::max);
            let level = ((g / peak) * (bars.len() - 1) as f64).round() as usize;
            out.push(bars[level.min(bars.len() - 1)]);
        }
        out
    }
}

/// The occupant load criterion, in g.
///
/// An occupant sits free at the start of an impact, still travelling at the speed the car was
/// doing, while the car slows underneath them. They cover 65 mm of slack before the belt takes
/// up, then a further 235 mm while it stretches. The criterion is the constant deceleration that
/// would bring them back to the car's speed within that 300 mm.
///
/// It is a better measure than peak deceleration because it accounts for how long the pulse
/// lasts, not just how hard it spikes. It can exceed the peak: a very short, very hard pulse
/// stops the car while the occupant is still moving, and everything they have left must then be
/// taken out in what remains of the 300 mm.
pub fn occupant_load_criterion(pulse: &Pulse, speed: f64) -> f64 {
    if pulse.samples.len() < 3 || speed <= 0.0 {
        return 0.0;
    }

    // The vehicle's own speed and distance through the impact.
    let mut trace: Vec<(f64, f64, f64)> = Vec::with_capacity(pulse.samples.len() + 200);
    let mut v = speed;
    let mut x = 0.0;
    let mut last_t = pulse.samples[0].0;
    for (t, g) in &pulse.samples {
        let dt = (t - last_t).max(0.0);
        v = (v - g * 9.81 * dt).max(0.0);
        x += v * dt;
        trace.push((*t, v, x));
        last_t = *t;
    }
    // The occupant keeps moving after the car has stopped, so the trace has to continue past
    // the end of the pulse or the criterion is never reached.
    let (mut t, v_end, mut x_end) = *trace.last().unwrap();
    for _ in 0..400 {
        t += 0.001;
        x_end += v_end * 0.001;
        trace.push((t, v_end, x_end));
    }

    let free = 0.065_f64;
    let restrained = 0.235_f64;

    // When the slack runs out: relative displacement reaches 65 mm.
    let t1 = trace
        .iter()
        .find(|(t, _, x)| speed * t - x >= free)
        .map(|(t, _, _)| *t);
    let Some(t1) = t1 else {
        // The car never slowed enough to use up the slack.
        return 0.0;
    };

    // From there the occupant decelerates at a constant rate that brings them back to the
    // vehicle's speed. Find the moment at which that costs the whole 300 mm.
    for (t, v, x) in trace.iter().filter(|(t, _, _)| *t > t1) {
        let dt = t - t1;
        if dt <= 1e-6 {
            continue;
        }
        let a = (speed - v) / dt;
        // Where the occupant is, having travelled at the impact speed to t1 and then slowed
        // steadily at `a`.
        let occupant = speed * t - 0.5 * a * dt * dt;
        if occupant - x >= free + restrained {
            return a / 9.81;
        }
    }
    // The occupant never used the full travel, so the belt never reached its limit. The
    // criterion is what it took to match the vehicle at the end.
    let (t_end, v_final, _) = *trace.last().unwrap();
    ((speed - v_final) / (t_end - t1).max(1e-6)) / 9.81
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square_pulse(g: f64, duration: f64) -> Pulse {
        let n = 500;
        Pulse {
            samples: (0..n)
                .map(|i| (duration * i as f64 / n as f64, g))
                .collect(),
            speed: g * 9.81 * duration,
        }
    }

    #[test]
    fn a_harder_pulse_gives_a_worse_criterion() {
        // The whole point of the measure: the same speed change over less time is worse for the
        // occupant, even though both stop the car.
        let gentle = square_pulse(15.0, 0.10);
        let harsh = square_pulse(30.0, 0.05);
        let a = occupant_load_criterion(&gentle, gentle.speed);
        let b = occupant_load_criterion(&harsh, harsh.speed);
        assert!(
            b > a,
            "a 30 g pulse over 50 ms should be worse than 15 g over 100 ms: {b:.1} against {a:.1}"
        );
    }

    #[test]
    fn the_criterion_is_the_same_order_as_the_pulse() {
        // It is not bounded by the peak. A short hard pulse stops the car while the occupant is
        // still moving, and the rest has to come out of what is left of the belt travel, which
        // can cost more than the car itself felt. But it should be the same order of magnitude,
        // or the maths has gone wrong.
        let p = square_pulse(25.0, 0.08);
        let olc = occupant_load_criterion(&p, p.speed);
        assert!(
            (0.5 * p.peak()..2.0 * p.peak()).contains(&olc),
            "criterion of {olc:.1} g against a peak of {:.1} g is not the same order",
            p.peak()
        );
    }

    /// A pulse with a short spike on the front and a plateau behind it.
    fn spiky_pulse(spike: f64, spike_ms: f64, plateau: f64, duration: f64) -> Pulse {
        let n = 2000;
        let samples: Vec<(f64, f64)> = (0..n)
            .map(|i| {
                let t = duration * i as f64 / n as f64;
                (t, if t < spike_ms / 1e3 { spike } else { plateau })
            })
            .collect();
        let speed = samples
            .windows(2)
            .map(|w| w[0].1 * 9.81 * (w[1].0 - w[0].0))
            .sum();
        Pulse { samples, speed }
    }

    #[test]
    fn a_brief_spike_moves_the_peak_but_barely_moves_the_criterion() {
        // The defining property of the measure, and the reason it is used instead of the peak.
        // A short spike hurts an occupant far less than the same peak sustained, because it is
        // over before they have moved. Two pulses that bring the car to rest from the same
        // speed should give nearly the same criterion no matter how spiky one of them is.
        let flat = spiky_pulse(18.0, 0.0, 18.0, 0.072);
        let spiky = spiky_pulse(45.0, 6.0, 15.6, 0.072);
        let a = occupant_load_criterion(&flat, flat.speed);
        let b = occupant_load_criterion(&spiky, spiky.speed);
        assert!(
            (flat.speed - spiky.speed).abs() < 0.5,
            "the two pulses should stop the car from the same speed: {:.2} and {:.2} m/s",
            flat.speed,
            spiky.speed
        );
        assert!(
            spiky.peak() > flat.peak() * 2.0,
            "the spiky pulse should peak far higher"
        );
        assert!(
            (a - b).abs() / a < 0.25,
            "the criterion should barely notice the spike: {a:.1} g flat against {b:.1} g spiky,              while the peaks are {:.0} and {:.0} g",
            flat.peak(),
            spiky.peak()
        );
    }

    #[test]
    fn no_pulse_means_no_criterion() {
        let empty = Pulse::default();
        assert_eq!(occupant_load_criterion(&empty, 15.0), 0.0);
    }

    #[test]
    fn a_sparkline_has_the_width_it_was_asked_for() {
        let p = square_pulse(20.0, 0.06);
        assert_eq!(p.sparkline(40).chars().count(), 40);
    }
}
