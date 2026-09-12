//! Compliance rule evaluation.
//!
//! A rule pack is a versioned set of checks; this crate decides which of them apply to a
//! vehicle, evaluates the ones that can be evaluated, and says plainly what it could not decide
//! and why. The output is evidence for a person, never a certification.

pub mod facts;

use std::path::Path;

use indexmap::IndexMap;
use thiserror::Error;
use wmds_expr::{Value, eval};
use wmds_schema::{Evidence, RuleDef, RulePackDef, Severity};

pub use facts::{ChassisFacts, FactEnv, Facts, MateFacts, PartFacts, Tier0Facts};

/// The statement that goes on every report. WMDS-35.
pub const DISCLAIMER: &str = "This report is design evidence, not a certification. A vehicle is \
certified by an approved signatory on the basis of evidence, of which this is one part.";

#[derive(Error, Debug)]
pub enum RulesError {
    #[error("{path}: {message}")]
    Load { path: String, message: String },
}

/// What happened to one rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Status {
    /// The rule does not apply to this vehicle.
    NotApplicable,
    /// Checked and satisfied.
    Pass,
    /// Satisfied by calculation, but a physical test is still required.
    PassNeedsTest,
    /// Needs a simulation that has not been run, or a declaration nobody has made.
    NeedsInput,
    /// Only a physical test can decide this.
    NeedsPhysicalTest,
    /// The rule could not be evaluated: a fact it needs does not exist yet.
    Undecided,
    /// Checked and not satisfied.
    Fail,
}

impl Status {
    pub fn label(&self) -> &'static str {
        match self {
            Status::NotApplicable => "n/a",
            Status::Pass => "pass",
            Status::PassNeedsTest => "pass, test required",
            Status::NeedsInput => "needs input",
            Status::NeedsPhysicalTest => "needs physical test",
            Status::Undecided => "undecided",
            Status::Fail => "FAIL",
        }
    }

    /// Does this status stop the design being signed off?
    pub fn blocks(&self) -> bool {
        matches!(self, Status::Fail)
    }
}

#[derive(Debug, Clone)]
pub struct RuleOutcome {
    pub pack: String,
    pub id: String,
    pub title: String,
    pub source: String,
    pub evidence: Evidence,
    pub severity: Severity,
    pub status: Status,
    /// Why, in a sentence a person can act on.
    pub detail: String,
    /// True when nobody has confirmed this rule against its source document.
    pub unverified: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ComplianceReport {
    pub vehicle: String,
    pub packs: Vec<(String, String)>,
    pub outcomes: Vec<RuleOutcome>,
}

impl ComplianceReport {
    pub fn count(&self, s: Status) -> usize {
        self.outcomes.iter().filter(|o| o.status == s).count()
    }

    /// Rules that block sign-off.
    pub fn failures(&self) -> impl Iterator<Item = &RuleOutcome> {
        self.outcomes.iter().filter(|o| o.status.blocks())
    }

    pub fn is_clear(&self) -> bool {
        self.failures().count() == 0
    }

    /// Rules nobody has checked against the regulation they claim to come from.
    pub fn unverified(&self) -> usize {
        self.outcomes
            .iter()
            .filter(|o| o.unverified && o.status != Status::NotApplicable)
            .count()
    }
}

/// Load every `.rules.kdl` under a directory.
pub fn load_packs(dir: &Path) -> (Vec<RulePackDef>, Vec<(String, String)>) {
    let mut packs = Vec::new();
    let mut failures = Vec::new();
    let mut files = Vec::new();
    collect(dir, &mut files);
    for f in files {
        let name = f
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let Ok(src) = std::fs::read_to_string(&f) else {
            continue;
        };
        match wmds_schema::parse_rules(&name, &src) {
            Ok(p) => packs.push(p),
            Err(e) => failures.push((f.display().to_string(), format!("{e:?}"))),
        }
    }
    (packs, failures)
}

fn collect(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    if !dir.is_dir() {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<std::path::PathBuf> = rd.flatten().map(|e| e.path()).collect();
    paths.sort();
    for p in paths {
        if p.is_dir() {
            collect(&p, out);
        } else if p.to_string_lossy().ends_with(".rules.kdl") {
            out.push(p);
        }
    }
}

/// Evaluate the selected packs against a vehicle.
///
/// `selected` names the packs the vehicle asked for; a pack that is not named is skipped
/// entirely rather than silently applied.
pub fn evaluate(packs: &[RulePackDef], selected: &[String], facts: &Facts) -> ComplianceReport {
    let mut report = ComplianceReport {
        vehicle: facts.id.clone(),
        ..Default::default()
    };
    for pack in packs {
        if !selected.iter().any(|s| s == &pack.id) {
            continue;
        }
        report.packs.push((pack.id.clone(), pack.version.clone()));
        for rule in &pack.rules {
            report.outcomes.push(evaluate_rule(pack, rule, facts));
        }
    }
    report
}

fn evaluate_rule(pack: &RulePackDef, rule: &RuleDef, facts: &Facts) -> RuleOutcome {
    let unverified = rule.verified_by.trim().is_empty();
    let mut out = RuleOutcome {
        pack: pack.id.clone(),
        id: rule.id.clone(),
        title: rule.title.clone(),
        source: rule.source.clone(),
        evidence: rule.evidence,
        severity: rule.severity,
        status: Status::Undecided,
        detail: String::new(),
        unverified,
    };

    // Applicability first: a rule that does not apply is not a gap.
    if let Some(when) = &rule.applies_when {
        let env = FactEnv {
            facts,
            result: None,
        };
        match eval(when, &env) {
            Ok(Value::Bool(false)) => {
                out.status = Status::NotApplicable;
                return out;
            }
            Ok(Value::Bool(true)) => {}
            Ok(v) => {
                out.detail = format!(
                    "applies_when returned {} instead of a yes or no",
                    v.type_name()
                );
                return out;
            }
            Err(e) => {
                out.detail = format!("applies_when could not be evaluated: {e}");
                return out;
            }
        }
    }

    match rule.evidence {
        Evidence::PhysicalTest => {
            out.status = Status::NeedsPhysicalTest;
            out.detail = if rule.note.is_empty() {
                "only a physical test can decide this".to_string()
            } else {
                rule.note.clone()
            };
            return out;
        }
        Evidence::Declaration => {
            match facts.declarations.get(&rule.id) {
                Some(d) => {
                    out.status = Status::Pass;
                    out.detail = format!("declared: {d}");
                }
                None => {
                    out.status = Status::NeedsInput;
                    out.detail = "needs a declaration from the designer or the supplier".into();
                }
            }
            return out;
        }
        Evidence::Simulation => {
            let Some(sim) = &rule.simulation else {
                out.detail = "the rule asks for simulation evidence but names no simulation".into();
                return out;
            };
            let Some(result) = facts.simulations.get(sim) else {
                out.status = Status::NeedsInput;
                out.detail = format!("simulation `{sim}` has not been run");
                return out;
            };
            let env = FactEnv {
                facts,
                result: Some(result),
            };
            decide(&mut out, rule, &env);
            return out;
        }
        Evidence::Calculation => {
            let env = FactEnv {
                facts,
                result: None,
            };
            decide(&mut out, rule, &env);
        }
    }
    out
}

fn decide(out: &mut RuleOutcome, rule: &RuleDef, env: &FactEnv) {
    let Some(check) = &rule.check else {
        out.detail = "the rule has no check".into();
        return;
    };
    match eval(check, env) {
        Ok(Value::Bool(true)) => {
            out.status = if rule.physical_test_required {
                Status::PassNeedsTest
            } else {
                Status::Pass
            };
            if rule.physical_test_required {
                out.detail =
                    "the calculation is satisfied; the regulation still requires a physical test"
                        .into();
            }
        }
        Ok(Value::Bool(false)) => {
            out.status = Status::Fail;
            out.detail = if rule.on_fail.is_empty() {
                "the check was not satisfied".to_string()
            } else {
                rule.on_fail.clone()
            };
        }
        Ok(v) => {
            out.detail = format!(
                "the check returned {} instead of a yes or no",
                v.type_name()
            );
        }
        Err(e) => {
            // A rule that cannot be evaluated is reported as undecided rather than as a pass.
            // Silently passing a rule the software could not check is the worst possible
            // failure mode for this feature.
            out.status = Status::Undecided;
            out.detail = format!("could not be evaluated: {e}");
        }
    }
}

/// Render the report as text.
pub fn render_text(report: &ComplianceReport) -> String {
    use std::fmt::Write;
    let mut s = String::new();
    let _ = writeln!(s, "Compliance report for {}", report.vehicle);
    if report.packs.is_empty() {
        let _ = writeln!(
            s,
            "\nNo rule packs were applied. The vehicle names none, or none were found."
        );
        let _ = writeln!(s, "\n{DISCLAIMER}");
        return s;
    }
    let _ = writeln!(
        s,
        "Rule packs: {}",
        report
            .packs
            .iter()
            .map(|(i, v)| format!("{i} v{v}"))
            .collect::<Vec<_>>()
            .join(", ")
    );

    let counts = [
        (Status::Fail, "failing"),
        (Status::Undecided, "undecided"),
        (Status::NeedsInput, "need input"),
        (Status::NeedsPhysicalTest, "need a physical test"),
        (Status::PassNeedsTest, "pass but need a test"),
        (Status::Pass, "pass"),
        (Status::NotApplicable, "not applicable"),
    ];
    let _ = writeln!(s, "\nSummary");
    for (st, label) in counts {
        let n = report.count(st);
        if n > 0 {
            let _ = writeln!(s, "  {n:>3}  {label}");
        }
    }

    let mut by_pack: IndexMap<&str, Vec<&RuleOutcome>> = IndexMap::new();
    for o in &report.outcomes {
        by_pack.entry(o.pack.as_str()).or_default().push(o);
    }
    for (pack, outcomes) in by_pack {
        let _ = writeln!(s, "\n{pack}");
        for o in outcomes {
            if o.status == Status::NotApplicable {
                continue;
            }
            let _ = writeln!(s, "  {:<20} {:<22} {}", o.status.label(), o.id, o.title);
            if !o.detail.is_empty() {
                let _ = writeln!(s, "  {:<20} {}", "", o.detail);
            }
            if !o.source.is_empty() {
                let _ = writeln!(s, "  {:<20} source: {}", "", o.source);
            }
            if o.unverified {
                let _ = writeln!(s, "  {:<20} NOT VERIFIED against its source document", "");
            }
        }
        let na = report
            .outcomes
            .iter()
            .filter(|o| o.pack == pack && o.status == Status::NotApplicable)
            .count();
        if na > 0 {
            let _ = writeln!(s, "  ({na} rule(s) do not apply to this vehicle)");
        }
    }

    if report.unverified() > 0 {
        let _ = writeln!(
            s,
            "\n{} applicable rule(s) have not been checked against the regulation they cite. Treat \
             their results as provisional.",
            report.unverified()
        );
    }
    let _ = writeln!(s, "\n{DISCLAIMER}");
    s
}

/// Render the report as JSON, for CI and for a signatory's own tooling.
pub fn render_json(report: &ComplianceReport) -> String {
    fn esc(s: &str) -> String {
        s.replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
    }
    let mut out = String::from("{\n");
    out.push_str(&format!("  \"vehicle\": \"{}\",\n", esc(&report.vehicle)));
    out.push_str(&format!("  \"disclaimer\": \"{}\",\n", esc(DISCLAIMER)));
    out.push_str("  \"packs\": [");
    out.push_str(
        &report
            .packs
            .iter()
            .map(|(i, v)| format!("{{\"id\": \"{}\", \"version\": \"{}\"}}", esc(i), esc(v)))
            .collect::<Vec<_>>()
            .join(", "),
    );
    out.push_str("],\n  \"rules\": [\n");
    let rows: Vec<String> = report
        .outcomes
        .iter()
        .map(|o| {
            format!(
                "    {{\"pack\": \"{}\", \"id\": \"{}\", \"title\": \"{}\", \"status\": \"{}\", \
                 \"evidence\": \"{}\", \"severity\": \"{}\", \"detail\": \"{}\", \"source\": \"{}\", \
                 \"verified\": {}}}",
                esc(&o.pack),
                esc(&o.id),
                esc(&o.title),
                o.status.label(),
                o.evidence.name(),
                o.severity.name(),
                esc(&o.detail),
                esc(&o.source),
                !o.unverified
            )
        })
        .collect();
    out.push_str(&rows.join(",\n"));
    out.push_str("\n  ]\n}\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pack(src: &str) -> RulePackDef {
        wmds_schema::parse_rules("t.rules.kdl", src)
            .map_err(|e| format!("{e:?}"))
            .unwrap()
    }

    fn facts() -> Facts {
        Facts {
            id: "test/car".into(),
            category: "MA".into(),
            modelled_mass: 300.0,
            point_mass: [("kerb".to_string(), 500.0), ("laden".to_string(), 160.0)]
                .into_iter()
                .collect(),
            chassis: Some(ChassisFacts {
                mass: 60.0,
                length: 3.0,
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    const SRC: &str = r#"
rulepack "test" version="1.0" {
    rule "mass-fraction" {
        title "Chassis mass fraction"
        verified "Someone" date="2026-01-01"
        evidence "calculation"
        check "chassis.mass <= 0.15 * vehicle.mass.kerb"
        on_fail "too heavy"
        severity "fail"
    }
    rule "only-for-trucks" {
        title "Truck only"
        applies_when "vehicle.category == \"NA\""
        evidence "calculation"
        check "false"
        severity "fail"
    }
    rule "needs-a-sim" {
        title "Braking"
        evidence "simulation" sim="braking.straight-line"
        check "result.stopping_distance <= 70 m"
        severity "fail"
    }
    rule "cannot-know" {
        title "Something we have no fact for"
        evidence "calculation"
        check "vehicle.wheelbase > 2 m"
        severity "fail"
    }
}
"#;

    #[test]
    fn applies_evaluates_and_reports_honestly() {
        let p = pack(SRC);
        let f = facts();
        let r = evaluate(&[p], &["test".to_string()], &f);

        let by = |id: &str| r.outcomes.iter().find(|o| o.id == id).unwrap().clone();

        // 60 kg chassis against an 800 kg kerb mass is 7.5 percent, inside the limit.
        assert_eq!(by("mass-fraction").status, Status::Pass);
        // Category MA, so the truck rule does not apply and is not counted as a gap.
        assert_eq!(by("only-for-trucks").status, Status::NotApplicable);
        // The simulation has not been run.
        assert_eq!(by("needs-a-sim").status, Status::NeedsInput);
        // The fact does not exist. This must never be reported as a pass.
        assert_eq!(by("cannot-know").status, Status::Undecided);
        assert!(by("cannot-know").detail.contains("could not be evaluated"));
        assert!(r.is_clear(), "nothing here should block");
    }

    #[test]
    fn a_breached_limit_fails_with_the_authors_explanation() {
        let p = pack(SRC);
        let mut f = facts();
        f.chassis.as_mut().unwrap().mass = 400.0;
        let r = evaluate(&[p], &["test".to_string()], &f);
        let o = r.outcomes.iter().find(|o| o.id == "mass-fraction").unwrap();
        assert_eq!(o.status, Status::Fail);
        assert_eq!(o.detail, "too heavy");
        assert!(!r.is_clear());
        assert_eq!(r.failures().count(), 1);
    }

    #[test]
    fn a_pack_the_vehicle_did_not_ask_for_is_not_applied() {
        let p = pack(SRC);
        let r = evaluate(&[p], &["some-other-pack".to_string()], &facts());
        assert!(r.outcomes.is_empty());
        assert!(r.packs.is_empty());
    }

    #[test]
    fn unverified_rules_are_counted() {
        let src = SRC.replace("        verified \"Someone\" date=\"2026-01-01\"\n", "");
        let r = evaluate(&[pack(&src)], &["test".to_string()], &facts());
        assert!(r.unverified() >= 1);
        assert!(render_text(&r).contains("NOT VERIFIED"));
    }

    #[test]
    fn simulation_results_are_used_when_present() {
        let p = pack(SRC);
        let mut f = facts();
        f.simulations.insert(
            "braking.straight-line".into(),
            [(
                "stopping_distance".to_string(),
                wmds_units::Quantity::from_unit(42.0, "m").unwrap(),
            )]
            .into_iter()
            .collect(),
        );
        let r = evaluate(&[p], &["test".to_string()], &f);
        assert_eq!(
            r.outcomes
                .iter()
                .find(|o| o.id == "needs-a-sim")
                .unwrap()
                .status,
            Status::Pass
        );
    }

    #[test]
    fn every_report_carries_the_disclaimer() {
        let r = evaluate(&[pack(SRC)], &["test".to_string()], &facts());
        assert!(render_text(&r).contains("not a certification"));
        assert!(render_json(&r).contains("not a certification"));
    }
}
