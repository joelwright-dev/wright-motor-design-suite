//! Reporting a crash result, with the caveats attached to it rather than in a footnote.

use std::fmt::Write;

use crate::{CrashModel, CrashResult};

pub fn write_text(model: &CrashModel, results: &[CrashResult]) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "{}  crash screening", model.vehicle);
    let _ = writeln!(
        s,
        "  {:.0} kg, occupant compartment starts {:.0} mm from the front",
        model.mass,
        (model.compartment_front - model.slices.first().map(|x| x.x).unwrap_or(0.0)) * 1e3
    );

    let _ = writeln!(s, "\nthe structure in front of the occupants");
    if model.elements.is_empty() {
        let _ = writeln!(
            s,
            "  none. No longitudinal member in the model has crush behaviour, so there is"
        );
        let _ = writeln!(
            s,
            "  nothing between the barrier and the people. That is the result, not an error."
        );
    }
    for e in &model.elements {
        let _ = writeln!(
            s,
            "  {:<22} {:>6.0} kN steady, {:>6.0} kN peak, {:>5.0} mm of stroke, {:>5.1} kJ",
            e.name,
            e.force / 1e3,
            e.peak_force / 1e3,
            e.stroke * 1e3,
            e.capacity / 1e3
        );
        for (mat, f) in &e.contributions {
            let _ = writeln!(s, "  {:<22}   {mat} gives {:.0} kN", "", f / 1e3);
        }
    }

    for r in results {
        let _ = writeln!(
            s,
            "\n{} at {:.0} km/h",
            r.standard.name(),
            r.speed_kph
        );
        let _ = writeln!(s, "  {}", r.standard.source());
        let _ = writeln!(
            s,
            "  energy           {:.0} kJ coming in, {:.0} kJ the structure can take",
            r.energy / 1e3,
            r.capacity / 1e3
        );
        let _ = writeln!(s, "  crush            {:.0} mm", r.crush * 1e3);
        if r.bottomed_out {
            let _ = writeln!(
                s,
                "  BOTTOMED OUT     the crush zone packed solid and the compartment took the rest"
            );
            let _ = writeln!(
                s,
                "  intrusion        {:.0} mm into the occupant compartment",
                r.intrusion * 1e3
            );
        } else {
            let _ = writeln!(
                s,
                "  intrusion        none: it stopped before the crush zone ran out"
            );
        }
        let _ = writeln!(s, "  peak             {:.0} g", r.peak_g);
        let _ = writeln!(s, "  mean             {:.0} g over {:.0} ms", r.mean_g, r.duration * 1e3);
        let _ = writeln!(
            s,
            "  occupant load    {:.0} g   (the constant rate a belted occupant would feel)",
            r.olc
        );
        let _ = writeln!(s, "  pulse            {}", r.pulse.sparkline(58));
        let _ = writeln!(
            s,
            "                   0 {:>54.0} ms",
            r.duration * 1e3
        );
        let verdict = verdict(r);
        let _ = writeln!(s, "  verdict          {verdict}");
        for n in &r.notes {
            let _ = writeln!(s, "  note             {n}");
        }
    }

    let _ = writeln!(s, "\nwhat this is");
    let _ = writeln!(
        s,
        "  A lumped mass model: the vehicle cut into slices joined by crushable elements, run"
    );
    let _ = writeln!(
        s,
        "  explicitly into a barrier. It answers whether there is enough crush length and what"
    );
    let _ = writeln!(
        s,
        "  pulse that gives, while the layout is still moving. It is not finite element, it has"
    );
    let _ = writeln!(
        s,
        "  no buckling modes, no joint failures and no contact, and it cannot tell you whether"
    );
    let _ = writeln!(
        s,
        "  a structure will fold the way it is meant to. Nothing here is certification evidence."
    );
    s
}

fn verdict(r: &CrashResult) -> String {
    if r.bottomed_out {
        return format!(
            "FAILS. The crush zone ran out and {:.0} mm went into the compartment.",
            r.intrusion * 1e3
        );
    }
    if r.olc > 40.0 {
        return format!(
            "Survivable structure, but {:.0} g of occupant load is a severe pulse. A softer \
             front would help.",
            r.olc
        );
    }
    if r.olc > 25.0 {
        return format!(
            "The compartment holds and the pulse is firm at {:.0} g. Typical of a small car.",
            r.olc
        );
    }
    format!(
        "The compartment holds and the pulse is gentle at {:.0} g. Check the structure is not \
         softer than it needs to be.",
        r.olc
    )
}
