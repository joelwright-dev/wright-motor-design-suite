//! Editing a primitive: the operations behind making a new part without writing a file.
//!
//! The brief asks for new primitives and new chassis systems to be buildable without additional
//! programming. That is only true if the application can create parameters, geometry and ports,
//! and write the result back. This module is the model half of that; the user interface calls
//! into it and does no editing of its own.
//!
//! Two things here are deliberately data driven rather than hard coded in the interface:
//!
//! * `FEATURES` says what arguments each geometry operation takes, so the editor can offer the
//!   right fields for a cylinder without knowing what a cylinder is.
//! * Port parameters come from the port type registry in `library/ports.kdl`, so adding a new
//!   kind of joint to that file gives the editor its fields for free.

use indexmap::IndexMap;
use wmds_expr::Expr;
use wmds_model::Library;
use wmds_schema::{
    AxisSpec, Feature, GeometryLevel, MassPropsDef, MfgMethodDef, ParamDef, PortDef, PrimitiveDef,
    VariantDef,
};

/// A geometry operation the editor can offer, and the arguments it takes.
pub struct FeatureKind {
    pub op: &'static str,
    pub doc: &'static str,
    /// Argument name, a hint for the field, and a starting value.
    pub args: &'static [(&'static str, &'static str, &'static str)],
    /// True when the operation combines bodies that were built earlier by name.
    pub boolean: bool,
}

/// Everything that can go in a geometry block.
///
/// The starting values are deliberately whole millimetres that produce something visible, so a
/// newly added feature shows up in the viewport instead of being an invisible zero-sized body
/// that leaves the person wondering whether the button worked.
pub const FEATURES: &[FeatureKind] = &[
    FeatureKind {
        op: "box",
        doc: "A rectangular block, centred on `at`",
        args: &[
            ("size", "length, width, height", "(100 mm, 100 mm, 100 mm)"),
            ("at", "centre", "(0 mm, 0 mm, 0 mm)"),
        ],
        boolean: false,
    },
    FeatureKind {
        op: "cylinder",
        doc: "A solid cylinder, centred on `at`, along `axis`",
        args: &[
            ("d", "diameter", "50 mm"),
            ("h", "height", "100 mm"),
            ("at", "centre", "(0 mm, 0 mm, 0 mm)"),
            ("axis", "x, y or z", "z"),
        ],
        boolean: false,
    },
    FeatureKind {
        op: "tube",
        doc: "A hollow tube running between two points. The only feature that can run at an angle",
        args: &[
            ("od", "outside diameter", "30 mm"),
            ("wall", "wall thickness", "3 mm"),
            ("from", "start", "(0 mm, 0 mm, 0 mm)"),
            ("to", "end", "(200 mm, 0 mm, 0 mm)"),
        ],
        boolean: false,
    },
    FeatureKind {
        op: "box_tube",
        doc: "A rectangular hollow section, centred on `at`",
        args: &[
            ("size", "length, width, height", "(500 mm, 60 mm, 120 mm)"),
            ("wall", "wall thickness", "3 mm"),
            ("at", "centre", "(0 mm, 0 mm, 0 mm)"),
        ],
        boolean: false,
    },
    FeatureKind {
        op: "union",
        doc: "Fuse everything built so far into one body. Usually the last line",
        args: &[],
        boolean: false,
    },
    FeatureKind {
        op: "subtract",
        doc: "Cut body b out of body a. Needs the real geometry kernel",
        args: &[("a", "body to cut", ""), ("b", "body to cut with", "")],
        boolean: true,
    },
    FeatureKind {
        op: "intersect",
        doc: "Keep only what a and b share. Needs the real geometry kernel",
        args: &[("a", "first body", ""), ("b", "second body", "")],
        boolean: true,
    },
    FeatureKind {
        op: "mirror",
        doc: "Copy a body reflected about a plane through the origin",
        args: &[("of", "body to mirror", ""), ("plane", "xy, xz or yz", "xz")],
        boolean: true,
    },
];

pub fn feature_kind(op: &str) -> Option<&'static FeatureKind> {
    FEATURES.iter().find(|f| f.op == op)
}

/// A new, empty part to start from.
///
/// It arrives with one parameter, one box and no ports, because an empty part builds nothing and
/// looks broken. A box with a length is something you can immediately see and drag.
pub fn new_primitive(id: &str) -> PrimitiveDef {
    let category = id.split('/').next().unwrap_or("misc").to_string();
    PrimitiveDef {
        id: id.to_string(),
        version: "0.1.0".into(),
        description: String::new(),
        category,
        sub: None,
        params: vec![
            ParamDef {
                name: "length".into(),
                unit: Some("mm".into()),
                default: Some(text_expr("200 mm")),
                min: Some(text_expr("20 mm")),
                max: Some(text_expr("2000 mm")),
                expr: None,
                doc: Some("overall length".into()),
            },
            ParamDef {
                name: "width".into(),
                unit: Some("mm".into()),
                default: Some(text_expr("60 mm")),
                min: Some(text_expr("5 mm")),
                max: Some(text_expr("1000 mm")),
                expr: None,
                doc: Some("overall width".into()),
            },
            ParamDef {
                name: "height".into(),
                unit: Some("mm".into()),
                default: Some(text_expr("40 mm")),
                min: Some(text_expr("5 mm")),
                max: Some(text_expr("1000 mm")),
                expr: None,
                doc: Some("overall height".into()),
            },
        ],
        variants: Vec::new(),
        material: Some("steel/s355-plate".into()),
        geometry: vec![GeometryLevel {
            level: "manufacture".into(),
            features: vec![Feature {
                op: "box".into(),
                name: Some("body".into()),
                args: args_of(&[
                    ("size", "(length, width, height)"),
                    ("at", "(0 mm, 0 mm, 0 mm)"),
                ]),
                positional: Vec::new(),
            }],
        }],
        massprops: MassPropsDef::Computed,
        ports: Vec::new(),
        behaviour: None,
        manufacturing: vec![MfgMethodDef {
            method: "machined".into(),
            scale: Some("1..*".into()),
            exports: vec!["step".into()],
            cost: None,
        }],
        compliance_tags: Vec::new(),
    }
}

fn args_of(pairs: &[(&str, &str)]) -> IndexMap<String, Expr> {
    let mut m = IndexMap::new();
    for (k, v) in pairs {
        m.insert(k.to_string(), text_expr(v));
    }
    m
}

/// Text a person typed, kept as text and parsed if it parses.
///
/// Keeping the original text is what makes `36 kWh` stay `36 kWh` in the saved file instead of
/// becoming a number of joules.
pub fn text_expr(text: &str) -> Expr {
    match wmds_expr::parse(text) {
        Ok(Expr::Str(s)) => Expr::Str(s),
        Ok(parsed) => Expr::TextOr(text.to_string(), Box::new(parsed)),
        Err(_) => Expr::Str(text.to_string()),
    }
}

// ------------------------------------------------------------------------------- parameters

fn unique(name: &str, taken: &[String]) -> String {
    if !taken.iter().any(|t| t == name) {
        return name.to_string();
    }
    for n in 2.. {
        let c = format!("{name}_{n}");
        if !taken.iter().any(|t| *t == c) {
            return c;
        }
    }
    unreachable!()
}

pub fn add_param(def: &mut PrimitiveDef) -> String {
    let taken: Vec<String> = def.params.iter().map(|p| p.name.clone()).collect();
    let name = unique("new_dimension", &taken);
    def.params.push(ParamDef {
        name: name.clone(),
        unit: Some("mm".into()),
        default: Some(text_expr("100 mm")),
        min: Some(text_expr("1 mm")),
        max: Some(text_expr("1000 mm")),
        expr: None,
        doc: None,
    });
    name
}

/// Remove a parameter, unless something still refers to it.
///
/// Returns the places that still use it. Deleting a parameter a geometry feature depends on
/// produces a part that will not resolve, and the failure appears far from the cause, so it is
/// refused here instead.
pub fn remove_param(def: &mut PrimitiveDef, name: &str) -> Result<(), Vec<String>> {
    let uses = param_uses(def, name);
    if !uses.is_empty() {
        return Err(uses);
    }
    def.params.retain(|p| p.name != name);
    Ok(())
}

/// Everywhere a parameter name appears in the rest of the part.
pub fn param_uses(def: &PrimitiveDef, name: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mentions = |e: &Expr| expr_mentions(e, name);
    for p in &def.params {
        if p.name == name {
            continue;
        }
        for (what, e) in [
            ("default", &p.default),
            ("min", &p.min),
            ("max", &p.max),
            ("expr", &p.expr),
        ] {
            if e.as_ref().is_some_and(mentions) {
                out.push(format!("parameter {} ({what})", p.name));
            }
        }
    }
    for lvl in &def.geometry {
        for f in &lvl.features {
            for (k, e) in &f.args {
                if mentions(e) {
                    out.push(format!(
                        "{} {} ({k})",
                        f.op,
                        f.name.clone().unwrap_or_default()
                    ));
                }
            }
        }
    }
    for p in &def.ports {
        if mentions(&p.at) {
            out.push(format!("port {} (at)", p.name));
        }
        for (k, e) in &p.params {
            if mentions(e) {
                out.push(format!("port {} ({k})", p.name));
            }
        }
    }
    if let MassPropsDef::Declared { mass, cg, .. } = &def.massprops {
        if mentions(mass) {
            out.push("declared mass".into());
        }
        if cg.as_ref().is_some_and(mentions) {
            out.push("declared centre of gravity".into());
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Does this expression read the named value anywhere inside it?
fn expr_mentions(e: &Expr, name: &str) -> bool {
    match e {
        Expr::Path(p) => p.first().is_some_and(|s| s == name),
        Expr::TextOr(_, inner) => expr_mentions(inner, name),
        Expr::Unary(_, a) => expr_mentions(a, name),
        Expr::Binary(_, a, b) => expr_mentions(a, name) || expr_mentions(b, name),
        Expr::Tuple(items) | Expr::List(items) => items.iter().any(|x| expr_mentions(x, name)),
        Expr::If(c, a, b) => {
            expr_mentions(c, name) || expr_mentions(a, name) || expr_mentions(b, name)
        }
        Expr::Call(_, args) => args.iter().any(|x| expr_mentions(x, name)),
        Expr::Member(base, _) => expr_mentions(base, name),
        Expr::MethodCall(base, _, args) => {
            expr_mentions(base, name) || args.iter().any(|x| expr_mentions(x, name))
        }
        Expr::Lambda(_, body) => expr_mentions(body, name),
        _ => false,
    }
}

/// Rename a parameter and every reference to it.
pub fn rename_param(def: &mut PrimitiveDef, from: &str, to: &str) {
    if from == to || to.trim().is_empty() {
        return;
    }
    for p in &mut def.params {
        if p.name == from {
            p.name = to.to_string();
        }
        for e in [&mut p.default, &mut p.min, &mut p.max, &mut p.expr]
            .into_iter()
            .flatten()
        {
            rename_in(e, from, to);
        }
    }
    for lvl in &mut def.geometry {
        for f in &mut lvl.features {
            for e in f.args.values_mut() {
                rename_in(e, from, to);
            }
        }
    }
    for p in &mut def.ports {
        rename_in(&mut p.at, from, to);
        for e in p.params.values_mut() {
            rename_in(e, from, to);
        }
        if let AxisSpec::Expr(e) = &mut p.axis {
            rename_in(e, from, to);
        }
    }
    if let MassPropsDef::Declared { mass, cg, .. } = &mut def.massprops {
        rename_in(mass, from, to);
        if let Some(c) = cg {
            rename_in(c, from, to);
        }
    }
}

/// Rewrite an expression with one name changed.
///
/// Works on the text, because that is what gets written back to the file, and then reparses.
/// Only whole words are replaced, so renaming `w` does not corrupt `width`.
fn rename_in(e: &mut Expr, from: &str, to: &str) {
    let text = wmds_schema::expr_text(e);
    let renamed = replace_word(&text, from, to);
    if renamed != text {
        *e = text_expr(&renamed);
    }
}

fn replace_word(text: &str, from: &str, to: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let bytes: Vec<char> = text.chars().collect();
    let word = |c: char| c.is_alphanumeric() || c == '_';
    let mut i = 0;
    let f: Vec<char> = from.chars().collect();
    while i < bytes.len() {
        let ends = i + f.len();
        let matches = ends <= bytes.len()
            && bytes[i..ends] == f[..]
            && (i == 0 || !word(bytes[i - 1]))
            && (ends == bytes.len() || !word(bytes[ends]));
        if matches {
            out.push_str(to);
            i = ends;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    out
}

// --------------------------------------------------------------------------------- geometry

pub fn add_feature(def: &mut PrimitiveDef, level: usize, op: &str) -> Option<usize> {
    let kind = feature_kind(op)?;
    let lvl = def.geometry.get_mut(level)?;
    let taken: Vec<String> = lvl.features.iter().filter_map(|f| f.name.clone()).collect();
    let name = unique(op, &taken);
    let mut args = IndexMap::new();
    for (k, _, start) in kind.args {
        if !start.is_empty() {
            args.insert(k.to_string(), text_expr(start));
        }
    }
    // A union with no arguments fuses everything, so it belongs at the end and needs no name.
    let feature = Feature {
        op: op.to_string(),
        name: if op == "union" && kind.args.is_empty() {
            None
        } else {
            Some(name)
        },
        args,
        positional: Vec::new(),
    };
    // Keep a trailing bare `union` last, because it consumes whatever was built before it.
    let insert_at = match lvl
        .features
        .iter()
        .position(|f| f.op == "union" && f.args.is_empty())
    {
        Some(i) if op != "union" => i,
        _ => lvl.features.len(),
    };
    lvl.features.insert(insert_at, feature);
    Some(insert_at)
}

pub fn remove_feature(def: &mut PrimitiveDef, level: usize, index: usize) {
    if let Some(lvl) = def.geometry.get_mut(level)
        && index < lvl.features.len()
    {
        lvl.features.remove(index);
    }
}

/// Move a feature earlier or later. Order matters: a boolean can only name a body built before it.
pub fn move_feature(def: &mut PrimitiveDef, level: usize, index: usize, delta: isize) {
    let Some(lvl) = def.geometry.get_mut(level) else {
        return;
    };
    let target = index as isize + delta;
    if target < 0 || target as usize >= lvl.features.len() {
        return;
    }
    lvl.features.swap(index, target as usize);
}

pub fn set_feature_arg(def: &mut PrimitiveDef, level: usize, index: usize, key: &str, value: &str) {
    if let Some(f) = def
        .geometry
        .get_mut(level)
        .and_then(|l| l.features.get_mut(index))
    {
        if value.trim().is_empty() {
            f.args.shift_remove(key);
        } else {
            f.args.insert(key.to_string(), text_expr(value));
        }
    }
}

pub fn set_feature_name(def: &mut PrimitiveDef, level: usize, index: usize, name: &str) {
    if let Some(f) = def
        .geometry
        .get_mut(level)
        .and_then(|l| l.features.get_mut(index))
    {
        f.name = if name.trim().is_empty() {
            None
        } else {
            Some(name.trim().to_string())
        };
    }
}

/// The bodies a boolean at this position is allowed to name: the ones built before it.
pub fn bodies_before(def: &PrimitiveDef, level: usize, index: usize) -> Vec<String> {
    def.geometry
        .get(level)
        .map(|l| {
            l.features
                .iter()
                .take(index)
                .filter_map(|f| f.name.clone())
                .collect()
        })
        .unwrap_or_default()
}

// ------------------------------------------------------------------------------------ ports

/// One parameter a port of a given type has to carry.
pub struct PortParamSlot {
    pub name: String,
    /// What the registry says it is: a unit name, or `text`, `int` or `bool`.
    pub kind: String,
    pub optional: bool,
    /// A value to start from, so a newly added port is valid rather than empty.
    pub start: String,
}

/// The parameters a port of this type must carry, from the port registry.
///
/// This is what lets the editor offer the right fields for a joint it has never heard of: the
/// registry is a data file, and adding a port type to it is enough.
pub fn port_type_params(lib: &Library, port_type: &str) -> Vec<PortParamSlot> {
    let Some(t) = lib.port_types.get(port_type) else {
        return Vec::new();
    };
    t.params
        .iter()
        .map(|p| {
            let (kind, start) = match &p.kind {
                wmds_schema::ParamKind::Int => ("int".to_string(), "4".to_string()),
                wmds_schema::ParamKind::Text => ("text".to_string(), "M12".to_string()),
                wmds_schema::ParamKind::Bool => ("bool".to_string(), "#true".to_string()),
                wmds_schema::ParamKind::Quantity(u) => (u.clone(), port_param_start(u)),
            };
            PortParamSlot {
                name: p.name.clone(),
                kind,
                optional: p.optional,
                start,
            }
        })
        .collect()
}

/// A sensible starting value for a port parameter, given the unit the registry declares.
fn port_param_start(unit: &str) -> String {
    match unit {
        "mm" => "50 mm".into(),
        "inch" => "15 inch".into(),
        "A" => "200 A".into(),
        "N/mm" => "100 N/mm".into(),
        "kN" => "10 kN".into(),
        u => format!("1 {u}"),
    }
}

pub fn add_port(def: &mut PrimitiveDef, lib: &Library, port_type: &str) -> String {
    let taken: Vec<String> = def.ports.iter().map(|p| p.name.clone()).collect();
    let base = port_type.split('.').next_back().unwrap_or("port").replace('-', "_");
    let name = unique(&base, &taken);
    let mut params = IndexMap::new();
    for slot in port_type_params(lib, port_type) {
        // An optional parameter is left out rather than guessed at.
        if !slot.optional {
            params.insert(slot.name, text_expr(&slot.start));
        }
    }
    def.ports.push(PortDef {
        name: name.clone(),
        port_type: port_type.to_string(),
        at: text_expr("(0 mm, 0 mm, 0 mm)"),
        axis: AxisSpec::Named("z".into()),
        // Declared from the start: a fixed joint whose ports have no clock has an arbitrary
        // rotation about the mating axis, and that mistake is invisible in the numbers.
        clock: Some(AxisSpec::Named("x".into())),
        symmetry: None,
        params,
        load_rating: None,
        grid: false,
    });
    name
}

pub fn remove_port(def: &mut PrimitiveDef, name: &str) {
    def.ports.retain(|p| p.name != name);
}

pub fn set_port_field(def: &mut PrimitiveDef, name: &str, field: &str, value: &str) {
    let Some(p) = def.ports.iter_mut().find(|p| p.name == name) else {
        return;
    };
    match field {
        "name" if !value.trim().is_empty() => p.name = value.trim().to_string(),
        "at" => p.at = text_expr(value),
        "axis" => p.axis = axis_from(value),
        "clock" => {
            p.clock = if value.trim().is_empty() {
                None
            } else {
                Some(axis_from(value))
            }
        }
        _ => {}
    }
}

pub fn set_port_param(def: &mut PrimitiveDef, name: &str, key: &str, value: &str) {
    if let Some(p) = def.ports.iter_mut().find(|p| p.name == name) {
        if value.trim().is_empty() {
            p.params.shift_remove(key);
        } else {
            p.params.insert(key.to_string(), text_expr(value));
        }
    }
}

/// Change a port's type, keeping any parameters the new type also wants.
pub fn set_port_type(def: &mut PrimitiveDef, lib: &Library, name: &str, port_type: &str) {
    let wanted = port_type_params(lib, port_type);
    let Some(p) = def.ports.iter_mut().find(|p| p.name == name) else {
        return;
    };
    p.port_type = port_type.to_string();
    let mut params = IndexMap::new();
    for slot in wanted {
        match p.params.get(&slot.name).cloned() {
            Some(v) => {
                params.insert(slot.name, v);
            }
            None if !slot.optional => {
                params.insert(slot.name.clone(), text_expr(&slot.start));
            }
            None => {}
        }
    }
    p.params = params;
}

fn axis_from(v: &str) -> AxisSpec {
    let t = v.trim();
    if matches!(t, "x" | "y" | "z" | "-x" | "-y" | "-z") {
        AxisSpec::Named(t.to_string())
    } else {
        AxisSpec::Expr(text_expr(t))
    }
}

pub fn axis_text(a: &AxisSpec) -> String {
    match a {
        AxisSpec::Named(n) => n.clone(),
        AxisSpec::Expr(e) => wmds_schema::expr_text(e),
    }
}

// --------------------------------------------------------------------------------- variants

pub fn add_variant(def: &mut PrimitiveDef) -> String {
    let taken: Vec<String> = def.variants.iter().map(|v| v.name.clone()).collect();
    let name = unique("side", &taken);
    def.variants.push(VariantDef {
        name: name.clone(),
        options: vec!["left".into(), "right".into()],
        mirror_when: Some("right".into()),
        mirror_plane: "xz".into(),
    });
    name
}

pub fn remove_variant(def: &mut PrimitiveDef, name: &str) {
    def.variants.retain(|v| v.name != name);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_part_is_valid_and_builds_something() {
        let def = new_primitive("test/bracket");
        let text = wmds_schema::write_primitive(&def);
        let back = wmds_schema::parse_primitive("t.prim.kdl", &text)
            .unwrap_or_else(|e| panic!("a new part is not a valid file:\n{text}\n{e:?}"));
        assert_eq!(back.id, "test/bracket");
        assert_eq!(back.category, "test");
        assert_eq!(back.geometry.len(), 1);
        assert_eq!(back.geometry[0].features.len(), 1);
        let r = wmds_model::resolve(&back, &wmds_model::Overrides::default())
            .expect("a new part must resolve straight away");
        assert_eq!(r.params.len(), 3);
    }

    #[test]
    fn a_parameter_in_use_cannot_be_deleted_by_accident() {
        let mut def = new_primitive("test/bracket");
        // `length` is used by the starting box.
        let err = remove_param(&mut def, "length").unwrap_err();
        assert!(
            err.iter().any(|u| u.contains("box")),
            "expected the box to be named as a user, got {err:?}"
        );
        assert_eq!(def.params.len(), 3, "nothing should have been removed");

        // One that nothing refers to goes cleanly.
        let name = add_param(&mut def);
        assert!(remove_param(&mut def, &name).is_ok());
    }

    #[test]
    fn renaming_a_parameter_rewrites_every_use_of_it() {
        let mut def = new_primitive("test/bracket");
        rename_param(&mut def, "length", "span");
        assert!(def.params.iter().any(|p| p.name == "span"));
        assert!(!def.params.iter().any(|p| p.name == "length"));
        let size = wmds_schema::expr_text(&def.geometry[0].features[0].args["size"]);
        assert!(
            size.contains("span") && !size.contains("length"),
            "the box still refers to the old name: {size}"
        );
        // And the part still resolves, which is the thing a rename usually breaks.
        let text = wmds_schema::write_primitive(&def);
        let back = wmds_schema::parse_primitive("t.prim.kdl", &text).expect("parses");
        wmds_model::resolve(&back, &wmds_model::Overrides::default()).expect("resolves");
    }

    #[test]
    fn renaming_does_not_corrupt_a_longer_name_that_contains_it() {
        // Renaming `w` must not turn `width` into `spanidth`.
        assert_eq!(replace_word("w * width + w", "w", "span"), "span * width + span");
        assert_eq!(replace_word("(w, h, d)", "w", "wide"), "(wide, h, d)");
    }

    #[test]
    fn a_trailing_union_stays_last() {
        let mut def = new_primitive("test/bracket");
        add_feature(&mut def, 0, "union");
        let added = add_feature(&mut def, 0, "cylinder").expect("added");
        let ops: Vec<&str> = def.geometry[0]
            .features
            .iter()
            .map(|f| f.op.as_str())
            .collect();
        assert_eq!(
            ops.last(),
            Some(&"union"),
            "a bare union fuses what came before it, so it has to stay last: {ops:?}"
        );
        assert!(added < def.geometry[0].features.len() - 1);
    }
}
