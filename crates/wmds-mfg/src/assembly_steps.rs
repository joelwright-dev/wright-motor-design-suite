//! Turning a mate graph into instructions somebody can follow.
//!
//! The order is not invented. A joint can only be made once both things it joins are in front of
//! you, and the placement solver already worked out which joint put each part where. Following
//! that gives an order that is buildable by construction: nothing is ever bolted to something
//! that has not been put down yet.
//!
//! Sub-assemblies come first and as a group, which is also how a flatpack works. You build the
//! four corners on the bench, then bolt the corners to the chassis.

use std::collections::{HashMap, HashSet};

use wmds_model::{PlacedBy, ResolvedAssembly, ResolvedMate};
use wmds_schema::Stage;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepKind {
    /// Put a part down to build from. Nothing is attached to it yet.
    Start,
    /// Join two things together.
    Join,
    /// A joint that does not position anything, because both ends are already placed. It is
    /// still a bolt to fit, and if it does not line up something upstream is wrong.
    AlsoBolt,
    /// Comes already assembled, made in the factory rather than by the person building the kit.
    Supplied,
}

#[derive(Debug, Clone)]
pub struct Step {
    pub number: usize,
    /// Which sub-assembly this belongs to, or none for the vehicle itself.
    pub group: Option<String>,
    pub kind: StepKind,
    pub text: String,
    /// What to fit it with.
    pub fastener: Option<String>,
    pub torque: Option<String>,
    /// The joint this step makes, for tracing back into the model.
    pub mate: Option<String>,
    /// Things that will go wrong here if something upstream is not right.
    pub warning: Option<String>,
}

/// Build the instructions for a resolved vehicle or assembly.
pub fn assembly_steps(asm: &ResolvedAssembly) -> Vec<Step> {
    // What placed each thing a joint can name. Sub-assembly parts carry the joint inside the
    // sub-assembly that positioned them; whole sub-assemblies carry the joint in the vehicle
    // that positioned them. A joint at either level has to find its answer here.
    let mut placed_by: HashMap<&str, &str> = HashMap::new();
    let mut roots_of: HashMap<Option<String>, Vec<&str>> = HashMap::new();
    for u in &asm.mateable {
        match &u.placed_by {
            PlacedBy::Mate(m) => {
                placed_by.insert(u.unit.as_str(), m.as_str());
            }
            PlacedBy::Root | PlacedBy::Free(_) => {
                roots_of.entry(None).or_default().push(u.unit.as_str());
            }
            PlacedBy::Unreached => {}
        }
    }
    for i in &asm.instances {
        let Some((unit, _)) = i.id.split_once('.') else {
            continue;
        };
        match &i.placed_by {
            PlacedBy::Mate(m) => {
                placed_by.insert(i.id.as_str(), m.as_str());
            }
            PlacedBy::Root | PlacedBy::Free(_) => roots_of
                .entry(Some(unit.to_string()))
                .or_default()
                .push(i.id.as_str()),
            PlacedBy::Unreached => {}
        }
    }

    // Group the work: each sub-assembly is built on the bench first, then the vehicle.
    let mut groups: Vec<Option<String>> = Vec::new();
    for m in &asm.mates {
        let g = m.unit.clone();
        if !groups.contains(&g) {
            groups.push(g);
        }
    }
    // Sub-assemblies before the vehicle's own work.
    groups.sort_by_key(|g| g.is_none());

    let mut steps: Vec<Step> = Vec::new();
    let mut done: HashSet<String> = HashSet::new();

    for group in &groups {
        let in_group: Vec<&ResolvedMate> =
            asm.mates.iter().filter(|m| &m.unit == group).collect();
        if in_group.is_empty() {
            continue;
        }

        let title = match group {
            Some(g) => format!("Build the {}", readable(g)),
            None => "Put it together".to_string(),
        };
        steps.push(Step {
            number: 0,
            group: group.clone(),
            kind: StepKind::Start,
            text: title,
            fastener: None,
            torque: None,
            mate: None,
            warning: None,
        });

        // What everything in this group is built from: the thing no joint had to place.
        let roots = roots_of.get(group).cloned().unwrap_or_default();
        for r in &roots {
            steps.push(Step {
                number: 0,
                group: group.clone(),
                kind: StepKind::Start,
                text: format!("Lay out {} to build on.", readable(r)),
                fastener: None,
                torque: None,
                mate: None,
                warning: None,
            });
            done.insert((*r).to_string());
        }

        // Then every joint that placed something, in the order the parts were reached.
        let mut remaining: Vec<&ResolvedMate> = in_group.clone();
        let mut guard = remaining.len() + 1;
        while !remaining.is_empty() && guard > 0 {
            guard -= 1;
            let mut progressed = false;
            remaining.retain(|m| {
                // Is this the joint that positioned one of its two ends?
                let places_a = placed_by.get(m.a.as_str()) == Some(&m.id.as_str());
                let places_b = placed_by.get(m.b.as_str()) == Some(&m.id.as_str());
                let ready = match (places_a, places_b) {
                    (true, _) => done.contains(&m.b),
                    (_, true) => done.contains(&m.a),
                    // Neither end depends on this joint: it is a second fixing, and it waits
                    // until both ends exist.
                    _ => done.contains(&m.a) && done.contains(&m.b),
                };
                if !ready {
                    return true;
                }
                progressed = true;
                let kind = if places_a || places_b {
                    StepKind::Join
                } else {
                    StepKind::AlsoBolt
                };
                steps.push(step_for(m, kind, group.clone()));
                done.insert(m.a.clone());
                done.insert(m.b.clone());
                false
            });
            if !progressed {
                break;
            }
        }
        // Anything left could not be ordered, which means the model has a loop the solver did
        // not resolve. Say so rather than dropping the joint.
        for m in remaining {
            let mut s = step_for(m, StepKind::AlsoBolt, group.clone());
            s.warning = Some(
                "This joint could not be placed in a build order, which usually means neither \
                 end is reachable from the part you started with."
                    .into(),
            );
            steps.push(s);
        }
    }

    for (i, s) in steps.iter_mut().enumerate() {
        s.number = i + 1;
    }
    steps
}

fn step_for(m: &ResolvedMate, kind: StepKind, group: Option<String>) -> Step {
    let kind = if m.stage == Stage::Factory {
        StepKind::Supplied
    } else {
        kind
    };
    let text = match kind {
        StepKind::Supplied => format!(
            "{} comes already fitted to {}.",
            readable(&m.b),
            readable(&m.a)
        ),
        StepKind::AlsoBolt => format!(
            "Also bolt {} at {} to {} at {}. It should line up without forcing.",
            readable(&m.b),
            m.b_port,
            readable(&m.a),
            m.a_port
        ),
        _ => format!(
            "Fit {} to {} at {}.",
            readable(&m.b),
            readable(&m.a),
            m.a_port
        ),
    };
    let fastener = m.fasteners.as_ref().map(|f| {
        let mut s = format!("{} x {} {} {}", f.quantity, f.kind, f.size, f.grade);
        if let Some(n) = &f.nut {
            s.push_str(&format!(", {n} nut"));
        }
        if let Some(w) = &f.washer {
            s.push_str(&format!(", {w} washer"));
        }
        if let Some(t) = &f.thread_locker {
            s.push_str(&format!(", {t} thread locker"));
        }
        s
    });
    let torque = m
        .fasteners
        .as_ref()
        .and_then(|f| f.torque.as_ref())
        .map(wmds_schema::expr_text);
    let warning = match (&kind, &m.fasteners) {
        (StepKind::Supplied, _) => None,
        (_, None) => Some("This joint has no fastener specified.".into()),
        (_, Some(f)) if f.torque.is_none() => {
            Some(format!("No torque figure for the {} {}.", f.size, f.kind))
        }
        _ => None,
    };
    Step {
        number: 0,
        group,
        kind,
        text,
        fastener,
        torque,
        mate: Some(m.id.clone()),
        warning,
    }
}

/// Turn an identifier into something readable in an instruction.
///
/// `corner_fl.lower_arm` becomes "front left corner lower arm". Nobody assembling a vehicle
/// should have to read an identifier, and a position marker reads better in front of the thing
/// it describes than tacked on the end of it.
fn readable(id: &str) -> String {
    id.split('.')
        .map(segment)
        .collect::<Vec<_>>()
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// One dotted segment, with any position markers moved to the front in the order written.
///
/// `bracket_upper_front` is an upper front bracket, not a front bracket upper. Markers are only
/// taken from the ends, so a word in the middle that happens to look like one is left alone.
fn segment(seg: &str) -> String {
    let words: Vec<&str> = seg.split('_').collect();
    let mut lead = 0;
    while lead < words.len() && position_word(words[lead]).is_some() {
        lead += 1;
    }
    let mut tail = words.len();
    while tail > lead && position_word(words[tail - 1]).is_some() {
        tail -= 1;
    }
    if lead == 0 && tail == words.len() {
        return words.iter().map(|w| expand(w)).collect::<Vec<_>>().join(" ");
    }
    if lead >= tail {
        // Every word is a position marker, so there is nothing to move it in front of.
        return words
            .iter()
            .map(|w| position_word(w).unwrap_or(w).to_string())
            .collect::<Vec<_>>()
            .join(" ");
    }
    let positions: Vec<String> = words[..lead]
        .iter()
        .chain(words[tail..].iter())
        .map(|w| position_word(w).unwrap_or(w).to_string())
        .collect();
    let body: Vec<String> = words[lead..tail].iter().map(|w| expand(w)).collect();
    format!("{} {}", positions.join(" "), body.join(" "))
}

/// Words that say where on the vehicle something is, rather than what it is.
fn position_word(w: &str) -> Option<&'static str> {
    Some(match w {
        "fl" => "front left",
        "fr" => "front right",
        "rl" => "rear left",
        "rr" => "rear right",
        "l" | "left" => "left",
        "r" | "right" => "right",
        "front" => "front",
        "rear" => "rear",
        "upper" => "upper",
        "lower" => "lower",
        _ => return None,
    })
}

/// Abbreviations that mean nothing to somebody holding a spanner.
fn expand(w: &str) -> String {
    match w {
        "lca" => "lower control arm".to_string(),
        "bj" => "ball joint".to_string(),
        "hv" => "high voltage".to_string(),
        "od" => "outside diameter".to_string(),
        "dw" => "double wishbone".to_string(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_are_turned_into_english() {
        assert_eq!(readable("corner_fl.lower_arm"), "front left corner lower arm");
        assert_eq!(readable("battery"), "battery");
        assert_eq!(readable("tie_rod_l"), "left tie rod");
        assert_eq!(readable("bracket_upper_front"), "upper front bracket");
        assert_eq!(readable("master_cylinder"), "master cylinder");
    }
}
