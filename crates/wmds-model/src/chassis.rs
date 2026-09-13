//! Generating a chassis assembly from a chassis system definition.
//!
//! The generator is generic: it reads a `.chassis.kdl` file and a configuration choice and
//! produces rails, cross-members, section-joint ports and a mount grid. MCDSv1 is one such
//! file; a second chassis family needs another file, not another generator (WMDS-20).
//!
//! Coordinates follow doc 04: the origin is on the vehicle centreline, at the front
//! section-joint plane, in the plane of the rail top surfaces. X runs rearward, Y to the RIGHT,
//! Z up. Grid stations are numbered from the front joint plane, negative forward.

use indexmap::IndexMap;
use thiserror::Error;
use wmds_expr::Value;
use wmds_schema::{AssemblyKind, ChassisRef};
use wmds_units::{Dim, Quantity};

use crate::assembly::{PlacedBy, PlacedInstance, ResolvedAssembly};
use crate::library::Library;
use crate::transform::Transform;
use crate::{Overrides, ResolvedPort, resolve};

#[derive(Error, Debug, Clone, PartialEq)]
pub enum ChassisError {
    #[error("unknown chassis system `{0}`")]
    UnknownSystem(String),
    #[error("chassis `{system}` has no configuration `{name}`")]
    UnknownConfiguration { system: String, name: String },
    #[error("chassis `{system}` has no width config `{name}`")]
    UnknownWidth { system: String, name: String },
    #[error("chassis `{system}` has no rail section `{name}`")]
    UnknownRailSection { system: String, name: String },
    #[error("section `{kind}`: length {given} is outside the allowed {min} to {max}")]
    LengthOutOfRange {
        kind: String,
        given: Quantity,
        min: Quantity,
        max: Quantity,
    },
    #[error("section `{0}` needs a length")]
    MissingLength(String),
    #[error("section `{kind}`: length {given} is not a whole number of {pitch} grid steps")]
    OffGrid {
        kind: String,
        given: Quantity,
        pitch: Quantity,
    },
    #[error("the rail primitive `{0}` is not in the library")]
    MissingRail(String),
    #[error("{0}")]
    Build(String),
}

/// A generated chassis, plus the facts a vehicle needs about it.
pub struct GeneratedChassis {
    pub assembly: ResolvedAssembly,
    /// Overall length from the foremost to the rearmost rail end.
    pub length: Quantity,
    /// Outer width across the rails.
    pub width: Quantity,
    /// Section kind -> (x at its front face, x at its rear face), metres.
    pub sections: IndexMap<String, (f64, f64)>,
    pub grid_pitch: Quantity,
    /// Lowest and highest grid station index that exists.
    pub station_range: (i32, i32),
}

/// Build a chassis assembly.
pub fn generate(lib: &Library, req: &ChassisRef) -> Result<GeneratedChassis, ChassisError> {
    let def = lib
        .chassis
        .get(&req.system)
        .ok_or_else(|| ChassisError::UnknownSystem(req.system.clone()))?;

    let sections = def.configuration(&req.configuration).ok_or_else(|| {
        ChassisError::UnknownConfiguration {
            system: def.id.clone(),
            name: req.configuration.clone(),
        }
    })?;
    let inner_spacing = def
        .width(&req.width)
        .ok_or_else(|| ChassisError::UnknownWidth {
            system: def.id.clone(),
            name: req.width.clone(),
        })?;
    let rail =
        def.rail_section(&req.rail_section)
            .ok_or_else(|| ChassisError::UnknownRailSection {
                system: def.id.clone(),
                name: req.rail_section.clone(),
            })?;

    // Section lengths, validated against their kind's range and the grid.
    let pitch = def.grid_pitch;
    let mut lengths: IndexMap<String, f64> = IndexMap::new();
    for kind in sections {
        let expr = req
            .section_lengths
            .get(kind)
            .ok_or_else(|| ChassisError::MissingLength(kind.clone()))?;
        let q = literal_length(expr).ok_or_else(|| ChassisError::MissingLength(kind.clone()))?;
        let sk = &def.section_kinds[kind];
        if q.value < sk.length_min.value - 1e-9 || q.value > sk.length_max.value + 1e-9 {
            return Err(ChassisError::LengthOutOfRange {
                kind: kind.clone(),
                given: q,
                min: sk.length_min,
                max: sk.length_max,
            });
        }
        let steps = q.value / pitch.value;
        if (steps - steps.round()).abs() > 1e-6 {
            return Err(ChassisError::OffGrid {
                kind: kind.clone(),
                given: q,
                pitch,
            });
        }
        lengths.insert(kind.clone(), q.value);
    }

    // Longitudinal layout. The front joint plane is x = 0, so the front section lies at
    // negative x and everything behind it accumulates rearward.
    let front_len = lengths.get("front").copied().unwrap_or(0.0);
    let mut spans: IndexMap<String, (f64, f64)> = IndexMap::new();
    let mut x = -front_len;
    for kind in sections {
        let l = lengths[kind];
        spans.insert(kind.clone(), (x, x + l));
        x += l;
    }
    let rear_end = x;

    // Lateral and vertical placement of the rails.
    let rail_w = rail.width.value;
    let rail_h = rail.height.value;
    let y_offset = inner_spacing.value / 2.0 + rail_w / 2.0;
    let z_centre = -rail_h / 2.0;

    let rail_def = lib
        .primitive(&def.parts.rail)
        .ok_or_else(|| ChassisError::MissingRail(def.parts.rail.clone()))?;

    let mut instances: Vec<PlacedInstance> = Vec::new();
    let mut exports: IndexMap<String, (String, String)> = IndexMap::new();
    let mut warnings = Vec::new();

    for kind in sections {
        let (x0, x1) = spans[kind];
        let length = x1 - x0;
        // Y is positive to the vehicle's right. The frame is x rearward, y right, z up, which
        // is right-handed; calling positive y "left" made it left-handed and put the driver on
        // the wrong side of an Australian car for a while before anybody noticed.
        for (side, sign) in [("right", 1.0f64), ("left", -1.0f64)] {
            let id = format!("{kind}_rail_{side}");
            let mut o = Overrides::default();
            o.params.insert(
                "length".into(),
                Value::Num(Quantity::new(length, Dim::LENGTH)),
            );
            o.params.insert("height".into(), Value::Num(rail.height));
            o.params.insert("width".into(), Value::Num(rail.width));
            o.params.insert("wall".into(), Value::Num(rail.wall));
            let mut resolved = resolve(rail_def, &o)
                .map_err(|e| ChassisError::Build(format!("rail `{id}`: {e}")))?;

            // Station ports along this rail, at every grid pitch inside the section.
            let first = (x0 / pitch.value).round() as i32;
            let last = (x1 / pitch.value).round() as i32;
            for s in first..=last {
                // A station on the joint plane belongs to the section behind it, so that two
                // adjacent sections do not both claim it.
                if s == last && kind != sections.last().unwrap() {
                    continue;
                }
                let local_x = s as f64 * pitch.value - (x0 + x1) / 2.0;
                let port = ResolvedPort {
                    name: format!("station_{s}"),
                    port_type: "mcds.grid-station".into(),
                    origin: [
                        Quantity::new(local_x, Dim::LENGTH),
                        Quantity::new(0.0, Dim::LENGTH),
                        Quantity::new(rail_h / 2.0, Dim::LENGTH),
                    ],
                    axis: [0.0, 0.0, 1.0],
                    clock: Some([1.0, 0.0, 0.0]),
                    symmetry: None,
                    params: [
                        ("bolt".to_string(), Value::Str(def.station_bolt.clone())),
                        ("side".to_string(), Value::Str(side.to_string())),
                    ]
                    .into_iter()
                    .collect(),
                    load_rating: None,
                    grid: true,
                };
                exports.insert(
                    format!("station_{side}_{s}"),
                    (id.clone(), port.name.clone()),
                );
                resolved.ports.push(port);
            }

            // Section joint ports at the ends that carry a joint.
            let sk = &def.section_kinds[kind];
            for end in &sk.joints {
                let (local_x, axis) = match end.as_str() {
                    "front" => (-length / 2.0, [-1.0, 0.0, 0.0]),
                    _ => (length / 2.0, [1.0, 0.0, 0.0]),
                };
                let port = ResolvedPort {
                    name: format!("joint_{end}"),
                    port_type: "mcds.section-joint".into(),
                    origin: [
                        Quantity::new(local_x, Dim::LENGTH),
                        Quantity::new(0.0, Dim::LENGTH),
                        Quantity::new(0.0, Dim::LENGTH),
                    ],
                    axis,
                    clock: Some([0.0, 0.0, 1.0]),
                    symmetry: None,
                    params: [
                        ("width_config".to_string(), Value::Str(req.width.clone())),
                        ("rail_section".to_string(), Value::Str(rail.name.clone())),
                        ("generation".to_string(), Value::num(def.generation as f64)),
                    ]
                    .into_iter()
                    .collect(),
                    load_rating: None,
                    grid: false,
                };
                exports.insert(
                    format!("joint_{kind}_{side}_{end}"),
                    (id.clone(), port.name.clone()),
                );
                resolved.ports.push(port);
            }

            let placement = Transform::translation([(x0 + x1) / 2.0, sign * y_offset, z_centre]);
            instances.push(PlacedInstance {
                id: id.clone(),
                source_id: def.parts.rail.clone(),
                primitive: resolved,
                placement,
                placed_by: PlacedBy::Root,
            });
        }
    }

    // Cross-members: one at each section joint plane and then evenly inside each section, never
    // further apart than the system's maximum spacing.
    let mut crossmember_count = 0usize;
    if let Some(cm_def) = lib.primitive(&def.parts.crossmember) {
        let span = inner_spacing.value;
        for kind in sections {
            let (x0, x1) = spans[kind];
            let length = x1 - x0;
            let n = (length / def.crossmember_spacing_max.value).ceil().max(1.0) as usize;
            for i in 0..=n {
                let t = i as f64 / n as f64;
                let x = x0 + t * length;
                // Skip a duplicate at a shared joint plane.
                if i == 0 && kind != &sections[0] {
                    continue;
                }
                let id = format!("{kind}_crossmember_{i}");
                let mut o = Overrides::default();
                o.params.insert(
                    "length".into(),
                    Value::Num(Quantity::new(span, Dim::LENGTH)),
                );
                let resolved = match resolve(cm_def, &o) {
                    Ok(r) => r,
                    Err(e) => {
                        warnings.push(format!("cross-member `{id}`: {e}"));
                        continue;
                    }
                };
                instances.push(PlacedInstance {
                    id,
                    source_id: def.parts.crossmember.clone(),
                    primitive: resolved,
                    placement: Transform::translation([x, 0.0, z_centre]),
                    placed_by: PlacedBy::Root,
                });
                crossmember_count += 1;
            }
        }
    } else if !def.parts.crossmember.is_empty() {
        warnings.push(format!(
            "cross-member primitive `{}` is not in the library, so the chassis has none",
            def.parts.crossmember
        ));
    }
    if crossmember_count == 0 {
        warnings.push(
            "no cross-members were generated; torsional stiffness will be far too low".into(),
        );
    }

    let station_range = (
        (-front_len / pitch.value).round() as i32,
        (rear_end / pitch.value).round() as i32,
    );

    let assembly = ResolvedAssembly {
        id: format!("{}/{}/{}", def.id, req.configuration, req.width),
        version: def.version.clone(),
        kind: AssemblyKind::Assembly,
        instances,
        mates: Vec::new(),
        point_masses: Vec::new(),
        exports,
        // Filled in by the parent, which is where a chassis station becomes mateable.
        mateable: Vec::new(),
        warnings,
        errors: Vec::new(),
    };

    Ok(GeneratedChassis {
        assembly,
        length: Quantity::new(rear_end - (-front_len), Dim::LENGTH),
        width: Quantity::new(inner_spacing.value + 2.0 * rail_w, Dim::LENGTH),
        sections: spans,
        grid_pitch: pitch,
        station_range,
    })
}

/// Section lengths must be literals: a chassis length that depended on something else would
/// make the mount grid, and every station index on it, unstable.
fn literal_length(e: &wmds_expr::Expr) -> Option<Quantity> {
    match e {
        wmds_expr::Expr::Num(q) if q.dim == Dim::LENGTH => Some(*q),
        wmds_expr::Expr::TextOr(_, inner) => literal_length(inner),
        _ => None,
    }
}

/// The grid station index nearest to a longitudinal position.
pub fn station_at(chassis: &GeneratedChassis, x: Quantity) -> i32 {
    (x.value / chassis.grid_pitch.value).round() as i32
}

/// Longitudinal position of a grid station.
pub fn station_x(chassis: &GeneratedChassis, station: i32) -> Quantity {
    Quantity::new(station as f64 * chassis.grid_pitch.value, Dim::LENGTH)
}

/// Summary of the sections for reports.
pub fn describe_sections(c: &GeneratedChassis) -> Vec<(String, Quantity, Quantity, Quantity)> {
    c.sections
        .iter()
        .map(|(k, (x0, x1))| {
            (
                k.clone(),
                Quantity::new(*x0, Dim::LENGTH),
                Quantity::new(*x1, Dim::LENGTH),
                Quantity::new(x1 - x0, Dim::LENGTH),
            )
        })
        .collect()
}
