//! Writing definition files back out.
//!
//! Until this existed, WMDS could only read. Nothing the application did could be saved, which
//! meant a vehicle could only be authored in a text editor. This module is what makes an
//! interactive editor possible.
//!
//! The output is deliberately the same shape a person would write by hand: same indentation,
//! same ordering, one mate per block. A file that round-trips through the editor should still be
//! reviewable in a pull request.

use std::fmt::Write;

use wmds_expr::Expr;

use crate::{AssemblyDef, AssemblyKind, InstanceSource, ParamDef};

/// Serialise an assembly or vehicle to KDL.
pub fn write_assembly(def: &AssemblyDef) -> String {
    let mut s = String::new();
    let kind = match def.kind {
        AssemblyKind::Vehicle => "vehicle",
        AssemblyKind::Assembly => "assembly",
    };
    let _ = writeln!(s, "{kind} {} version={} {{", quote(&def.id), quote(&def.version));
    if !def.description.is_empty() {
        let _ = writeln!(s, "    description {}", quote(&def.description));
    }

    if let Some(v) = &def.vehicle {
        let _ = writeln!(s);
        if !v.category.is_empty() {
            let _ = writeln!(s, "    category {}", quote(&v.category));
        }
        if !v.markets.is_empty() {
            let _ = writeln!(s, "    markets {}", v.markets.iter().map(|m| quote(m)).collect::<Vec<_>>().join(" "));
        }
        if !v.rule_packs.is_empty() {
            let _ = writeln!(s, "    rule_packs {}", v.rule_packs.iter().map(|m| quote(m)).collect::<Vec<_>>().join(" "));
        }
    }

    if let Some(c) = &def.chassis {
        let _ = writeln!(s);
        let _ = writeln!(s, "    chassis system={} id={} {{", quote(&c.system), quote(&c.id));
        let _ = writeln!(s, "        configuration {}", quote(&c.configuration));
        let _ = writeln!(s, "        width {}", quote(&c.width));
        if !c.rail_section.is_empty() {
            let _ = writeln!(s, "        rail_section {}", quote(&c.rail_section));
        }
        for (kind, len) in &c.section_lengths {
            let _ = writeln!(s, "        section {} length={}", quote(kind), quote(&expr_text(len)));
        }
        let _ = writeln!(s, "    }}");
    }

    if !def.params.is_empty() {
        let _ = writeln!(s);
        let _ = writeln!(s, "    params {{");
        for p in &def.params {
            let _ = writeln!(s, "        {}", param_line(p));
        }
        let _ = writeln!(s, "    }}");
    }

    let _ = writeln!(s);
    let _ = writeln!(s, "    instances {{");
    for i in &def.instances {
        let source = match &i.source {
            InstanceSource::Primitive(p) => format!("primitive={}", quote(p)),
            InstanceSource::Assembly(a) => format!("assembly={}", quote(a)),
        };
        let version = i.version.as_ref().map(|v| format!(" version={}", quote(v))).unwrap_or_default();
        let has_body = !i.params.is_empty() || !i.variants.is_empty() || i.placement.is_some();
        if !has_body {
            let _ = writeln!(s, "        instance {} {source}{version}", quote(&i.id));
            continue;
        }
        let _ = writeln!(s, "        instance {} {source}{version} {{", quote(&i.id));
        if !i.params.is_empty() {
            let mut line = String::from("            set");
            for (k, v) in &i.params {
                let _ = write!(line, " {k}={}", quote(&expr_text(v)));
            }
            let _ = writeln!(s, "{line}");
        }
        if !i.variants.is_empty() {
            let mut line = String::from("            variant");
            for (k, v) in &i.variants {
                let _ = write!(line, " {k}={}", quote(&expr_text(v)));
            }
            let _ = writeln!(s, "{line}");
        }
        if let Some(p) = &i.placement {
            let mut line = format!("            place at={}", quote(&expr_text(&p.at)));
            if let Some(r) = &p.rotate {
                let _ = write!(line, " rotate={}", quote(&expr_text(r)));
            }
            if let Some(m) = &p.mirror {
                let _ = write!(line, " mirror={}", quote(m));
            }
            let _ = write!(line, " because={}", quote(&p.justification));
            let _ = writeln!(s, "{line}");
        }
        let _ = writeln!(s, "        }}");
    }
    let _ = writeln!(s, "    }}");

    if let Some(r) = &def.root {
        let _ = writeln!(s);
        let _ = writeln!(s, "    root {}", quote(r));
    }

    if !def.mates.is_empty() {
        let _ = writeln!(s);
        let _ = writeln!(s, "    mates {{");
        for m in &def.mates {
            let head = format!(
                "        mate {} a={} b={}",
                quote(&m.id),
                quote(&m.a.to_string()),
                quote(&m.b.to_string())
            );
            let extras = m.fasteners.is_some() || m.stage.is_some() || m.dof.is_some() || m.offset.is_some() || m.clock.is_some();
            if !extras {
                let _ = writeln!(s, "{head}");
                continue;
            }
            let mut head = head;
            if let Some(d) = m.dof {
                let _ = write!(head, " dof={}", quote(d.name()));
            }
            if let Some(o) = &m.offset {
                let _ = write!(head, " offset={}", quote(&expr_text(o)));
            }
            if let Some(c) = &m.clock {
                let _ = write!(head, " clock={}", quote(&expr_text(c)));
            }
            let _ = writeln!(s, "{head} {{");
            if let Some(f) = &m.fasteners {
                let mut line = format!("            fasteners kind={}", quote(&f.kind));
                if !f.size.is_empty() {
                    let _ = write!(line, " size={}", quote(&f.size));
                }
                if !f.grade.is_empty() {
                    let _ = write!(line, " grade={}", quote(&f.grade));
                }
                let _ = write!(line, " qty={}", quote(&f.quantity.to_string()));
                if let Some(t) = &f.torque {
                    let _ = write!(line, " torque={}", quote(&expr_text(t)));
                }
                if let Some(n) = &f.nut {
                    let _ = write!(line, " nut={}", quote(n));
                }
                if let Some(w) = &f.washer {
                    let _ = write!(line, " washer={}", quote(w));
                }
                if let Some(t) = &f.thread_locker {
                    let _ = write!(line, " thread_locker={}", quote(t));
                }
                let _ = writeln!(s, "{line}");
            }
            if let Some(st) = m.stage {
                let _ = writeln!(s, "            stage {}", quote(st.name()));
            }
            let _ = writeln!(s, "        }}");
        }
        let _ = writeln!(s, "    }}");
    }

    if !def.exports.is_empty() {
        let _ = writeln!(s);
        let _ = writeln!(s, "    ports {{");
        for e in &def.exports {
            let _ = writeln!(s, "        export {} as={}", quote(&e.source.to_string()), quote(&e.name));
        }
        let _ = writeln!(s, "    }}");
    }

    if !def.point_masses.is_empty() {
        let _ = writeln!(s);
        let _ = writeln!(s, "    masses {{");
        for p in &def.point_masses {
            let _ = writeln!(
                s,
                "        mass {} value={} at={} state={}",
                quote(&p.id),
                quote(&expr_text(&p.mass)),
                quote(&expr_text(&p.at)),
                quote(&p.state)
            );
        }
        let _ = writeln!(s, "    }}");
    }

    let _ = writeln!(s, "}}");
    s
}

fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

fn param_line(p: &ParamDef) -> String {
    let mut line = p.name.clone();
    if let Some(u) = &p.unit {
        let _ = write!(line, " unit={}", quote(u));
    }
    if let Some(e) = &p.expr {
        let _ = write!(line, " expr={}", quote(&expr_text(e)));
    } else if let Some(d) = &p.default {
        let _ = write!(line, " default={}", quote(&expr_text(d)));
    }
    if let Some(m) = &p.min {
        let _ = write!(line, " min={}", quote(&expr_text(m)));
    }
    if let Some(m) = &p.max {
        let _ = write!(line, " max={}", quote(&expr_text(m)));
    }
    if let Some(d) = &p.doc {
        let _ = write!(line, " doc={}", quote(d));
    }
    line
}

/// Turn an expression back into the text a person would have written.
///
/// Expressions that came from a file carry their original text, so they round-trip exactly.
/// Anything the editor built is printed from the tree.
pub fn expr_text(e: &Expr) -> String {
    match e {
        Expr::TextOr(t, _) => t.clone(),
        Expr::Str(t) => t.clone(),
        Expr::Num(q) => q.to_string(),
        Expr::Bool(b) => b.to_string(),
        Expr::Path(p) => p.join("."),
        Expr::Tuple(v) => format!("({})", v.iter().map(expr_text).collect::<Vec<_>>().join(", ")),
        Expr::List(v) => format!("[{}]", v.iter().map(expr_text).collect::<Vec<_>>().join(", ")),
        Expr::Unary(op, x) => match op {
            wmds_expr::UnOp::Neg => format!("-{}", expr_text(x)),
            wmds_expr::UnOp::Not => format!("not {}", expr_text(x)),
        },
        Expr::Binary(op, a, b) => {
            let o = match op {
                wmds_expr::BinOp::Add => "+",
                wmds_expr::BinOp::Sub => "-",
                wmds_expr::BinOp::Mul => "*",
                wmds_expr::BinOp::Div => "/",
                wmds_expr::BinOp::Pow => "^",
                wmds_expr::BinOp::Eq => "==",
                wmds_expr::BinOp::Ne => "!=",
                wmds_expr::BinOp::Lt => "<",
                wmds_expr::BinOp::Le => "<=",
                wmds_expr::BinOp::Gt => ">",
                wmds_expr::BinOp::Ge => ">=",
                wmds_expr::BinOp::And => "and",
                wmds_expr::BinOp::Or => "or",
                wmds_expr::BinOp::In => "in",
            };
            format!("{} {o} {}", expr_text(a), expr_text(b))
        }
        Expr::If(c, a, b) => format!("if {} then {} else {}", expr_text(c), expr_text(a), expr_text(b)),
        Expr::Call(n, args) => format!("{n}({})", args.iter().map(expr_text).collect::<Vec<_>>().join(", ")),
        Expr::Lambda(p, b) => format!("{p} -> {}", expr_text(b)),
        Expr::Member(b, n) => format!("{}.{n}", expr_text(b)),
        Expr::MethodCall(b, n, args) => format!(
            "{}.{n}({})",
            expr_text(b),
            args.iter().map(expr_text).collect::<Vec<_>>().join(", ")
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_assembly;

    const VEH: &str = r#"
vehicle "test/car" version="0.2.0" {
    description "A test"
    category "MA"
    markets "AU"
    rule_packs "wright-internal"
    chassis system="mcds-v1" id="chassis" {
        configuration "2/3-length"
        width "narrow"
        rail_section "120x60"
        section "front" length="1100 mm"
        section "central" length="1900 mm"
    }
    params {
        ride_height unit="mm" default="150" min="80" max="260" doc="how high it sits"
    }
    instances {
        instance "battery" primitive="energy/battery/pack-modular" {
            set energy="36 kWh" length="1200 mm"
            variant hand="left"
        }
        instance "drive" primitive="drivetrain/motor/drive-unit"
    }
    root "battery"
    mates {
        mate "m1" a="chassis.station_left_1" b="battery.mount_fl" {
            fasteners kind="bolt" size="M12" grade="8.8" qty="2" torque="90 Nm" nut="nyloc"
            stage "kit"
        }
        mate "m2" a="chassis.station_left_9" b="drive.mount_fl"
    }
    masses {
        mass "driver" value="80 kg" at="(600 mm, 350 mm, 750 mm)" state="laden"
    }
}
"#;

    #[test]
    fn a_vehicle_survives_a_round_trip() {
        let a = parse_assembly("t.veh.kdl", VEH).map_err(|e| format!("{e:?}")).unwrap();
        let text = write_assembly(&a);
        let b = parse_assembly("t.veh.kdl", &text)
            .map_err(|e| format!("writing produced something unparseable:\n{text}\n{e:?}"))
            .unwrap();

        assert_eq!(b.id, a.id);
        assert_eq!(b.version, a.version);
        assert_eq!(b.description, a.description);
        assert_eq!(b.kind, a.kind);
        assert_eq!(b.vehicle.as_ref().unwrap().category, "MA");
        assert_eq!(b.vehicle.as_ref().unwrap().rule_packs, vec!["wright-internal"]);
        assert_eq!(b.root, a.root);
        assert_eq!(b.instances.len(), a.instances.len());
        assert_eq!(b.mates.len(), a.mates.len());
        assert_eq!(b.point_masses.len(), 1);
        assert_eq!(b.params.len(), 1);

        let c = b.chassis.as_ref().unwrap();
        assert_eq!(c.system, "mcds-v1");
        assert_eq!(c.configuration, "2/3-length");
        assert_eq!(c.section_lengths.len(), 2);

        let f = b.mates[0].fasteners.as_ref().unwrap();
        assert_eq!(f.size, "M12");
        assert_eq!(f.quantity, 2);
        assert!(f.torque.is_some());
        assert_eq!(b.mates[0].stage, a.mates[0].stage);
        // A mate with nothing extra stays a one-liner rather than gaining an empty block.
        assert!(b.mates[1].fasteners.is_none());
    }

    #[test]
    fn writing_twice_gives_the_same_text() {
        // The editor saves repeatedly; a file that churns on every save is unreviewable.
        let a = parse_assembly("t.veh.kdl", VEH).unwrap();
        let once = write_assembly(&a);
        let b = parse_assembly("t.veh.kdl", &once).unwrap();
        let twice = write_assembly(&b);
        assert_eq!(once, twice);
    }

    #[test]
    fn expressions_keep_the_text_they_were_written_with() {
        let a = parse_assembly("t.veh.kdl", VEH).unwrap();
        let text = write_assembly(&a);
        assert!(text.contains("\"36 kWh\""), "{text}");
        assert!(text.contains("\"(600 mm, 350 mm, 750 mm)\""), "{text}");
        assert!(text.contains("\"1100 mm\""), "{text}");
    }
}
