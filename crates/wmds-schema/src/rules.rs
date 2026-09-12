//! Regulation and design rule packs (`.rules.kdl`).
//!
//! A rule pack is a versioned set of checks for one jurisdiction, standard or company policy.
//! Each rule says what it measures, when it applies, what kind of evidence satisfies it, and the
//! expression that decides. Rules are data so that a regulation change is a file edit.
//!
//! Nothing here certifies anything. A pack produces evidence for a person to sign.

use kdl::{KdlDocument, KdlNode};
use wmds_expr::Expr;

use crate::{
    Ctx, SchemaError, SchemaErrors, child, children, first_positional_string, prop_string,
};

/// What kind of evidence satisfies a rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Evidence {
    /// Checked directly against the model.
    Calculation,
    /// Requires a named simulation at a named tier.
    Simulation,
    /// Cannot be shown by software at all.
    PhysicalTest,
    /// A person or supplier declares it.
    Declaration,
}

impl Evidence {
    pub fn parse(s: &str) -> Option<Evidence> {
        Some(match s {
            "calculation" => Evidence::Calculation,
            "simulation" => Evidence::Simulation,
            "physical-test" => Evidence::PhysicalTest,
            "declaration" => Evidence::Declaration,
            _ => return None,
        })
    }

    pub fn name(&self) -> &'static str {
        match self {
            Evidence::Calculation => "calculation",
            Evidence::Simulation => "simulation",
            Evidence::PhysicalTest => "physical test",
            Evidence::Declaration => "declaration",
        }
    }
}

/// How badly a failure matters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// Worth knowing, not blocking.
    Advisory,
    /// Should be fixed before the design is considered done.
    Warning,
    /// The vehicle cannot be signed off like this.
    Fail,
}

impl Severity {
    pub fn parse(s: &str) -> Option<Severity> {
        Some(match s {
            "advisory" => Severity::Advisory,
            "warning" => Severity::Warning,
            "fail" => Severity::Fail,
            _ => return None,
        })
    }

    pub fn name(&self) -> &'static str {
        match self {
            Severity::Advisory => "advisory",
            Severity::Warning => "warning",
            Severity::Fail => "fail",
        }
    }
}

#[derive(Debug, Clone)]
pub struct RuleDef {
    pub id: String,
    pub title: String,
    /// The clause this rule comes from, so a reader can go and check it.
    pub source: String,
    /// Who confirmed the rule against its source, and when. Empty means nobody has.
    pub verified_by: String,
    pub verified_date: String,
    /// When this rule applies. Absent means always.
    pub applies_when: Option<Expr>,
    pub evidence: Evidence,
    /// For simulation evidence, the simulation that produces it.
    pub simulation: Option<String>,
    /// The test that must be done even when the calculation passes.
    pub physical_test_required: bool,
    pub check: Option<Expr>,
    pub on_fail: String,
    pub note: String,
    pub severity: Severity,
}

#[derive(Debug, Clone)]
pub struct RulePackDef {
    pub id: String,
    pub version: String,
    pub description: String,
    pub source: String,
    pub jurisdiction: String,
    pub rules: Vec<RuleDef>,
}

/// Parse a `.rules.kdl` file.
pub fn parse_rules(name: &str, src: &str) -> Result<RulePackDef, SchemaErrors> {
    let doc: KdlDocument = match src.parse() {
        Ok(d) => d,
        Err(e) => {
            let errors = e
                .diagnostics
                .iter()
                .map(|d| SchemaError {
                    msg: d.to_string(),
                    span: Some(d.span),
                })
                .collect();
            return Err(SchemaErrors::new(name, src, errors));
        }
    };
    let mut ctx = Ctx { errors: Vec::new() };
    let roots: Vec<&KdlNode> = doc
        .nodes()
        .iter()
        .filter(|n| n.name().value() == "rulepack")
        .collect();
    let def = match roots.as_slice() {
        [one] => parse_pack(&mut ctx, one),
        [] => {
            ctx.errors.push(SchemaError {
                msg: "file has no `rulepack` node".into(),
                span: None,
            });
            None
        }
        _ => {
            ctx.err(roots[1], "only one `rulepack` per file");
            None
        }
    };
    match def {
        Some(d) if ctx.errors.is_empty() => Ok(d),
        _ => Err(SchemaErrors::new(name, src, ctx.errors)),
    }
}

fn parse_pack(ctx: &mut Ctx, node: &KdlNode) -> Option<RulePackDef> {
    let id = match first_positional_string(node) {
        Some(s) => s,
        None => {
            ctx.err(
                node,
                "`rulepack` needs an id, e.g. rulepack \"adr/icv-vsb14-lo\"",
            );
            return None;
        }
    };
    let version = prop_string(node, "version").unwrap_or_else(|| "0.0.0".into());
    let description = child(node, "description")
        .and_then(first_positional_string)
        .unwrap_or_default();
    let source = child(node, "source")
        .and_then(first_positional_string)
        .unwrap_or_default();
    let jurisdiction = child(node, "jurisdiction")
        .and_then(first_positional_string)
        .unwrap_or_default();

    let mut rules = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for n in children(node).iter().filter(|n| n.name().value() == "rule") {
        let Some(rid) = first_positional_string(n) else {
            ctx.err(n, "rule needs an id");
            continue;
        };
        if !seen.insert(rid.clone()) {
            ctx.err(n, format!("duplicate rule id `{rid}`"));
        }
        let title = child(n, "title")
            .and_then(first_positional_string)
            .unwrap_or_else(|| rid.clone());
        let evidence = match child(n, "evidence").and_then(first_positional_string) {
            Some(s) => match Evidence::parse(&s) {
                Some(e) => e,
                None => {
                    ctx.err(n, format!("rule `{rid}`: unknown evidence `{s}` (calculation, simulation, physical-test, declaration)"));
                    Evidence::Calculation
                }
            },
            None => {
                ctx.err(n, format!("rule `{rid}` needs an `evidence` class"));
                Evidence::Calculation
            }
        };
        let severity = match child(n, "severity").and_then(first_positional_string) {
            Some(s) => match Severity::parse(&s) {
                Some(v) => v,
                None => {
                    ctx.err(n, format!("rule `{rid}`: unknown severity `{s}`"));
                    Severity::Fail
                }
            },
            None => Severity::Fail,
        };
        let expr_of = |ctx: &mut Ctx, key: &str| -> Option<Expr> {
            let text = child(n, key).and_then(first_positional_string)?;
            match wmds_expr::parse(&text) {
                Ok(e) => Some(e),
                Err(err) => {
                    ctx.err(n, format!("rule `{rid}`: `{key}`: {err}"));
                    None
                }
            }
        };
        let applies_when = expr_of(ctx, "applies_when");
        let check = expr_of(ctx, "check");
        if check.is_none() && matches!(evidence, Evidence::Calculation | Evidence::Simulation) {
            ctx.err(
                n,
                format!("rule `{rid}`: {} evidence needs a `check`", evidence.name()),
            );
        }
        let (verified_by, verified_date) = match child(n, "verified") {
            Some(v) => (
                first_positional_string(v).unwrap_or_default(),
                prop_string(v, "date").unwrap_or_default(),
            ),
            None => (String::new(), String::new()),
        };
        let physical_test_required = child(n, "physical_test_required")
            .and_then(|p| p.entries().first().and_then(|e| e.value().as_bool()))
            .unwrap_or(evidence == Evidence::PhysicalTest);

        rules.push(RuleDef {
            id: rid,
            title,
            source: child(n, "source")
                .and_then(first_positional_string)
                .unwrap_or_default(),
            verified_by,
            verified_date,
            applies_when,
            evidence,
            simulation: child(n, "evidence").and_then(|e| prop_string(e, "sim")),
            physical_test_required,
            check,
            on_fail: child(n, "on_fail")
                .and_then(first_positional_string)
                .unwrap_or_default(),
            note: child(n, "note")
                .and_then(first_positional_string)
                .unwrap_or_default(),
            severity,
        });
    }
    if rules.is_empty() {
        ctx.err(node, "a rule pack with no rules checks nothing");
    }
    Some(RulePackDef {
        id,
        version,
        description,
        source,
        jurisdiction,
        rules,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = r#"
rulepack "wright-internal" version="0.1.0" {
    description "Company design rules"
    source "Wright Motor Company"
    jurisdiction "internal"

    rule "chassis.mass-fraction" {
        title "Chassis mass fraction"
        source "docs/04-mcds-v1-spec.md section 6"
        verified "J. Wright" date="2026-09-12"
        applies_when "vehicle.category in [\"MA\", \"MB\", \"MC\", \"NA\"]"
        evidence "calculation"
        check "chassis.mass <= 0.15 * vehicle.mass.kerb"
        on_fail "The chassis is too large a share of kerb mass."
        severity "fail"
    }

    rule "crash.frontal" {
        title "Full frontal impact"
        evidence "simulation" sim="crash.frontal-full-width"
        check "result.intrusion <= 150 mm"
        physical_test_required #true
        severity "fail"
    }
}
"#;

    #[test]
    fn parses_a_pack() {
        let p = parse_rules("x.rules.kdl", SRC)
            .map_err(|e| format!("{e:?}"))
            .unwrap();
        assert_eq!(p.id, "wright-internal");
        assert_eq!(p.rules.len(), 2);
        let r = &p.rules[0];
        assert_eq!(r.evidence, Evidence::Calculation);
        assert_eq!(r.severity, Severity::Fail);
        assert_eq!(r.verified_by, "J. Wright");
        assert!(r.applies_when.is_some() && r.check.is_some());
        assert!(!r.physical_test_required);
        let c = &p.rules[1];
        assert_eq!(c.evidence, Evidence::Simulation);
        assert_eq!(c.simulation.as_deref(), Some("crash.frontal-full-width"));
        assert!(c.physical_test_required);
    }

    #[test]
    fn a_calculation_rule_without_a_check_is_an_error() {
        let bad = SRC.replace(
            "        check \"chassis.mass <= 0.15 * vehicle.mass.kerb\"\n",
            "",
        );
        let e = parse_rules("x.rules.kdl", &bad).expect_err("should fail");
        assert!(
            e.errors.iter().any(|x| x.msg.contains("needs a `check`")),
            "{:?}",
            e.errors
        );
    }
}
