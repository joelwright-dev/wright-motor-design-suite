//! Rendering a build pack, as plain text for a terminal or Markdown for a document.

use std::fmt::Write;

use crate::{BuildPack, StepKind};

/// The whole pack as Markdown: what to make, what to buy, and how to put it together.
pub fn write_markdown(pack: &BuildPack) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "# {} v{}", pack.vehicle, pack.version);
    let _ = writeln!(s);
    let _ = writeln!(
        s,
        "Build pack for a production volume of {} vehicle(s).",
        pack.volume
    );
    let (unit, tooling) = pack.cost();
    let _ = writeln!(s);
    let _ = writeln!(s, "| | |");
    let _ = writeln!(s, "|---|---|");
    let _ = writeln!(s, "| Parts | {} |", pack.bom.len());
    let _ = writeln!(
        s,
        "| Pieces per vehicle | {} |",
        pack.bom.iter().map(|b| b.quantity).sum::<usize>()
    );
    let _ = writeln!(s, "| Modelled mass | {:.1} kg |", pack.total_mass());
    let _ = writeln!(
        s,
        "| Fasteners per vehicle | {} |",
        pack.fasteners.iter().map(|f| f.quantity).sum::<u32>()
    );
    let _ = writeln!(
        s,
        "| Assembly steps a person carries out | {} |",
        pack.steps
            .iter()
            .filter(|x| matches!(x.kind, StepKind::Join | StepKind::AlsoBolt))
            .count()
    );
    match unit {
        Some(u) => {
            let _ = writeln!(s, "| Parts cost per vehicle | {u:.0} AUD |");
        }
        None => {
            let _ = writeln!(s, "| Parts cost per vehicle | not known |");
        }
    }
    let _ = writeln!(s, "| Tooling assumed | {tooling:.0} AUD |");

    if !pack.caveats.is_empty() {
        let _ = writeln!(s);
        let _ = writeln!(s, "## Read this first");
        let _ = writeln!(s);
        for c in &pack.caveats {
            let _ = writeln!(s, "- {c}");
        }
    }

    let _ = writeln!(s);
    let _ = writeln!(s, "## Bill of materials");
    let _ = writeln!(s);
    let _ = writeln!(
        s,
        "| Part | Qty | Hands | Material | Each | Total | How it is made | Cost each |"
    );
    let _ = writeln!(s, "|---|---|---|---|---|---|---|---|");
    for b in &pack.bom {
        let hands = if b.variants.is_empty() {
            "-".to_string()
        } else {
            b.variants
                .iter()
                .map(|(h, n)| format!("{n} {h}"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let _ = writeln!(
            s,
            "| {} | {} | {} | {} | {} | {} | {} | {} |",
            b.source_id,
            b.quantity,
            hands,
            b.material.clone().unwrap_or_else(|| "-".into()),
            b.unit_mass
                .map(|m| format!("{m:.2} kg"))
                .unwrap_or_else(|| "-".into()),
            b.total_mass
                .map(|m| format!("{m:.2} kg"))
                .unwrap_or_else(|| "-".into()),
            b.plan
                .as_ref()
                .map(|p| p.method.clone())
                .unwrap_or_else(|| "no route".into()),
            b.plan
                .as_ref()
                .and_then(|p| p.unit_cost)
                .map(|c| format!("{c:.0} AUD"))
                .unwrap_or_else(|| "-".into()),
        );
    }

    if !pack.cuts.is_empty() {
        let _ = writeln!(s);
        let _ = writeln!(s, "## Cut list");
        let _ = writeln!(s);
        let _ = writeln!(s, "| Section | Length | Qty | For | Material |");
        let _ = writeln!(s, "|---|---|---|---|---|");
        for c in &pack.cuts {
            let _ = writeln!(
                s,
                "| {} | {:.0} mm | {} | {} ({}) | {} |",
                c.section,
                c.length * 1e3,
                c.quantity,
                c.source_id,
                c.body,
                c.material.clone().unwrap_or_else(|| "-".into())
            );
        }
        let total: f64 = pack.cuts.iter().map(|c| c.length * c.quantity as f64).sum();
        let _ = writeln!(s);
        let _ = writeln!(s, "{:.1} m of section per vehicle, before offcuts.", total);
    }

    let _ = writeln!(s);
    let _ = writeln!(s, "## Fasteners");
    let _ = writeln!(s);
    let _ = writeln!(s, "| Fastener | Qty | Torque | With | Fitted by |");
    let _ = writeln!(s, "|---|---|---|---|---|");
    for f in &pack.fasteners {
        let mut with = Vec::new();
        if let Some(n) = &f.nut {
            with.push(format!("{n} nut"));
        }
        if let Some(w) = &f.washer {
            with.push(format!("{w} washer"));
        }
        if let Some(t) = &f.thread_locker {
            with.push(format!("{t} thread locker"));
        }
        let _ = writeln!(
            s,
            "| {} {} {} | {} | {} | {} | {} |",
            f.kind,
            f.size,
            f.grade,
            f.quantity,
            f.torque
                .map(|t| format!("{t:.0} Nm"))
                .unwrap_or_else(|| "varies or missing".into()),
            if with.is_empty() {
                "-".to_string()
            } else {
                with.join(", ")
            },
            if f.kit { "you" } else { "the factory" }
        );
    }

    let _ = writeln!(s);
    let _ = writeln!(s, "## Assembly");
    let _ = writeln!(s);
    let _ = writeln!(
        s,
        "Follow these in order. Each step only asks for parts that are already in front of you."
    );
    let mut group: Option<Option<String>> = None;
    for step in &pack.steps {
        if group.as_ref() != Some(&step.group) {
            group = Some(step.group.clone());
            let _ = writeln!(s);
            let _ = writeln!(s, "### {}", step.text);
            let _ = writeln!(s);
            continue;
        }
        let mut line = format!("{}. {}", step.number, step.text);
        if let Some(f) = &step.fastener {
            let _ = write!(line, " Use {f}.");
        }
        if let Some(t) = &step.torque {
            let _ = write!(line, " Tighten to {t}.");
        }
        let _ = writeln!(s, "{line}");
        if let Some(w) = &step.warning {
            let _ = writeln!(s, "   - {w}");
        }
    }

    let _ = writeln!(s);
    let _ = writeln!(
        s,
        "This pack is generated from the model. It is not a substitute for a qualified \
         inspection of the finished vehicle."
    );
    s
}

/// A terser form for a terminal.
pub fn write_text(pack: &BuildPack) -> String {
    let mut s = String::new();
    let (unit, tooling) = pack.cost();
    let _ = writeln!(s, "{} v{}  at a volume of {}", pack.vehicle, pack.version, pack.volume);
    let _ = writeln!(
        s,
        "  {} distinct parts, {} pieces, {:.1} kg, {} fasteners, {} assembly steps",
        pack.bom.len(),
        pack.bom.iter().map(|b| b.quantity).sum::<usize>(),
        pack.total_mass(),
        pack.fasteners.iter().map(|f| f.quantity).sum::<u32>(),
        pack.steps
            .iter()
            .filter(|x| matches!(x.kind, StepKind::Join | StepKind::AlsoBolt))
            .count()
    );
    match unit {
        Some(u) => {
            let _ = writeln!(s, "  parts {u:.0} AUD per vehicle, tooling {tooling:.0} AUD");
        }
        None => {
            let _ = writeln!(s, "  cost not known; tooling {tooling:.0} AUD");
        }
    }

    let _ = writeln!(s, "\nbill of materials");
    for b in &pack.bom {
        let _ = writeln!(
            s,
            "  {:<42} x{:<4} {:>9}  {:<18} {}",
            b.source_id,
            b.quantity,
            b.total_mass
                .map(|m| format!("{m:.2} kg"))
                .unwrap_or_else(|| "-".into()),
            b.plan
                .as_ref()
                .map(|p| p.method.clone())
                .unwrap_or_else(|| "NO ROUTE".into()),
            b.plan
                .as_ref()
                .and_then(|p| p.unit_cost)
                .map(|c| format!("{c:.0} AUD each"))
                .unwrap_or_default()
        );
    }

    if !pack.cuts.is_empty() {
        let _ = writeln!(s, "\ncut list");
        for c in &pack.cuts {
            let _ = writeln!(
                s,
                "  {:<30} {:>8.0} mm  x{:<4} {}",
                c.section,
                c.length * 1e3,
                c.quantity,
                c.source_id
            );
        }
    }

    let _ = writeln!(s, "\nfasteners");
    for f in &pack.fasteners {
        let _ = writeln!(
            s,
            "  {:<8} {:<10} {:<6} x{:<4} {:<14} {}",
            f.kind,
            f.size,
            f.grade,
            f.quantity,
            f.torque
                .map(|t| format!("{t:.0} Nm"))
                .unwrap_or_else(|| "no torque".into()),
            if f.kit { "fitted by the owner" } else { "factory" }
        );
    }

    let _ = writeln!(s, "\nassembly");
    for step in &pack.steps {
        if step.mate.is_none() {
            let _ = writeln!(s, "\n  {}", step.text);
            continue;
        }
        let mut line = format!("  {:>3}. {}", step.number, step.text);
        if let Some(f) = &step.fastener {
            let _ = write!(line, " Use {f}.");
        }
        if let Some(t) = &step.torque {
            let _ = write!(line, " Tighten to {t}.");
        }
        let _ = writeln!(s, "{line}");
        if let Some(w) = &step.warning {
            let _ = writeln!(s, "       ! {w}");
        }
    }

    if !pack.caveats.is_empty() {
        let _ = writeln!(s, "\nwhat this pack cannot tell you");
        for c in &pack.caveats {
            let _ = writeln!(s, "  - {c}");
        }
    }
    s
}
