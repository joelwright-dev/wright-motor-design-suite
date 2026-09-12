//! Chassis system definitions (`.chassis.kdl`).
//!
//! A chassis system describes a family of chassis: the section kinds, their allowed lengths,
//! the width options, the rail cross-sections, and the mount grid. The generator in
//! `wmds-chassis` turns one of these plus a configuration choice into an assembly. MCDSv1 is a
//! data file, not code (WMDS-20).

use indexmap::IndexMap;
use kdl::{KdlDocument, KdlNode};
use wmds_units::Quantity;

use crate::{
    Ctx, SchemaError, SchemaErrors, child, children, first_positional_string, prop, prop_string,
    value_to_string,
};

#[derive(Debug, Clone)]
pub struct ChassisDef {
    pub id: String,
    pub version: String,
    pub description: String,
    pub grid_pitch: Quantity,
    pub generation: i64,
    pub width_configs: IndexMap<String, Quantity>,
    pub rail_sections: IndexMap<String, RailSectionDef>,
    pub section_kinds: IndexMap<String, SectionKindDef>,
    pub configurations: IndexMap<String, Vec<String>>,
    pub parts: ChassisParts,
    pub crossmember_spacing_max: Quantity,
    pub station_bolt: String,
}

#[derive(Debug, Clone)]
pub struct RailSectionDef {
    pub name: String,
    pub height: Quantity,
    pub width: Quantity,
    pub wall: Quantity,
    pub material: String,
}

#[derive(Debug, Clone)]
pub struct SectionKindDef {
    pub name: String,
    pub length_min: Quantity,
    pub length_max: Quantity,
    /// Which ends carry a section joint: `front`, `rear`, or both.
    pub joints: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ChassisParts {
    pub rail: String,
    pub crossmember: String,
    pub joint_fitting: String,
}

impl ChassisDef {
    pub fn width(&self, name: &str) -> Option<Quantity> {
        self.width_configs.get(name).copied()
    }

    pub fn rail_section(&self, name: &str) -> Option<&RailSectionDef> {
        self.rail_sections.get(name)
    }

    pub fn configuration(&self, name: &str) -> Option<&Vec<String>> {
        self.configurations.get(name)
    }
}

/// Parse a `.chassis.kdl` file.
pub fn parse_chassis(name: &str, src: &str) -> Result<ChassisDef, SchemaErrors> {
    let doc: KdlDocument = match src.parse() {
        Ok(d) => d,
        Err(e) => {
            let errors = e
                .diagnostics
                .iter()
                .map(|d| SchemaError { msg: d.to_string(), span: Some(d.span) })
                .collect();
            return Err(SchemaErrors::new(name, src, errors));
        }
    };
    let mut ctx = Ctx { errors: Vec::new() };
    let roots: Vec<&KdlNode> = doc.nodes().iter().filter(|n| n.name().value() == "chassis").collect();
    let def = match roots.as_slice() {
        [one] => parse_root(&mut ctx, one),
        [] => {
            ctx.errors.push(SchemaError { msg: "file has no `chassis` node".into(), span: None });
            None
        }
        _ => {
            ctx.err(roots[1], "only one `chassis` node per file");
            None
        }
    };
    match def {
        Some(d) if ctx.errors.is_empty() => Ok(d),
        _ => Err(SchemaErrors::new(name, src, ctx.errors)),
    }
}

/// Read a length-valued property, reporting a clear error if it is missing or wrong.
fn length(ctx: &mut Ctx, node: &KdlNode, key: &str) -> Option<Quantity> {
    let raw = prop(node, key)?;
    let text = value_to_string(raw);
    match Quantity::parse(&text) {
        Ok(q) if q.dim == wmds_units::Dim::LENGTH => Some(q),
        Ok(q) => {
            ctx.err(node, format!("`{key}` must be a length, found {q}"));
            None
        }
        Err(e) => {
            ctx.err(node, format!("`{key}`: {e}"));
            None
        }
    }
}

fn parse_root(ctx: &mut Ctx, node: &KdlNode) -> Option<ChassisDef> {
    let id = match first_positional_string(node) {
        Some(s) => s,
        None => {
            ctx.err(node, "`chassis` needs an id, e.g. chassis \"mcds-v1\"");
            return None;
        }
    };
    let version = prop_string(node, "version").unwrap_or_else(|| "0.0.0".into());
    let description = child(node, "description").and_then(first_positional_string).unwrap_or_default();

    let grid_pitch = child(node, "grid_pitch")
        .and_then(first_positional_string)
        .and_then(|s| Quantity::parse(&s).ok())
        .filter(|q| q.dim == wmds_units::Dim::LENGTH);
    let grid_pitch = match grid_pitch {
        Some(q) if q.value > 0.0 => q,
        _ => {
            ctx.err(node, "chassis needs a positive `grid_pitch`, e.g. grid_pitch \"100 mm\"");
            Quantity::from_unit(100.0, "mm").unwrap()
        }
    };

    let generation = child(node, "generation")
        .and_then(|g| g.entries().first().and_then(|e| e.value().as_integer()))
        .unwrap_or(1) as i64;

    let mut width_configs = IndexMap::new();
    match child(node, "width_configs") {
        Some(b) => {
            for n in children(b) {
                if let Some(q) = length(ctx, n, "inner_rail_spacing") {
                    width_configs.insert(n.name().value().to_string(), q);
                } else {
                    ctx.err(n, format!("width config `{}` needs inner_rail_spacing=", n.name().value()));
                }
            }
        }
        None => ctx.err(node, "missing `width_configs`"),
    }

    let mut rail_sections = IndexMap::new();
    match child(node, "rail_sections") {
        Some(b) => {
            for n in children(b) {
                let sname = first_positional_string(n).unwrap_or_else(|| n.name().value().to_string());
                let (h, w, wall) = (length(ctx, n, "height"), length(ctx, n, "width"), length(ctx, n, "wall"));
                let (Some(height), Some(width), Some(wall)) = (h, w, wall) else {
                    ctx.err(n, format!("rail section `{sname}` needs height=, width= and wall="));
                    continue;
                };
                if wall.value * 2.0 >= width.value.min(height.value) {
                    ctx.err(n, format!("rail section `{sname}`: wall is too thick for the section"));
                }
                rail_sections.insert(
                    sname.clone(),
                    RailSectionDef {
                        name: sname,
                        height,
                        width,
                        wall,
                        material: prop_string(n, "material").unwrap_or_default(),
                    },
                );
            }
        }
        None => ctx.err(node, "missing `rail_sections`"),
    }

    let mut section_kinds = IndexMap::new();
    match child(node, "section_kinds") {
        Some(b) => {
            for n in children(b) {
                let kname = n.name().value().to_string();
                let (Some(length_min), Some(length_max)) = (length(ctx, n, "length_min"), length(ctx, n, "length_max")) else {
                    ctx.err(n, format!("section kind `{kname}` needs length_min= and length_max="));
                    continue;
                };
                if length_min.value > length_max.value {
                    ctx.err(n, format!("section kind `{kname}`: length_min exceeds length_max"));
                }
                let joints: Vec<String> = prop_string(n, "joints")
                    .unwrap_or_default()
                    .split_whitespace()
                    .map(|s| s.to_string())
                    .collect();
                for j in &joints {
                    if j != "front" && j != "rear" {
                        ctx.err(n, format!("section kind `{kname}`: unknown joint end `{j}`"));
                    }
                }
                section_kinds.insert(kname.clone(), SectionKindDef { name: kname, length_min, length_max, joints });
            }
        }
        None => ctx.err(node, "missing `section_kinds`"),
    }

    let mut configurations = IndexMap::new();
    match child(node, "configurations") {
        Some(b) => {
            for n in children(b) {
                let cname = first_positional_string(n).unwrap_or_else(|| n.name().value().to_string());
                let sections: Vec<String> = prop_string(n, "sections")
                    .unwrap_or_default()
                    .split_whitespace()
                    .map(|s| s.to_string())
                    .collect();
                if sections.is_empty() {
                    ctx.err(n, format!("configuration `{cname}` needs sections=\"front central ...\""));
                }
                for s in &sections {
                    if !section_kinds.contains_key(s) {
                        ctx.err(n, format!("configuration `{cname}` uses unknown section kind `{s}`"));
                    }
                }
                configurations.insert(cname, sections);
            }
        }
        None => ctx.err(node, "missing `configurations`"),
    }

    let parts = match child(node, "parts") {
        Some(b) => ChassisParts {
            rail: child(b, "rail").and_then(|n| prop_string(n, "primitive")).unwrap_or_default(),
            crossmember: child(b, "crossmember").and_then(|n| prop_string(n, "primitive")).unwrap_or_default(),
            joint_fitting: child(b, "joint_fitting").and_then(|n| prop_string(n, "primitive")).unwrap_or_default(),
        },
        None => {
            ctx.err(node, "missing `parts` block naming the rail, crossmember and joint fitting primitives");
            ChassisParts::default()
        }
    };
    if parts.rail.is_empty() {
        ctx.err(node, "`parts` must name a rail primitive");
    }

    let crossmember_spacing_max = child(node, "crossmembers")
        .and_then(|b| length(ctx, b, "spacing_max"))
        .unwrap_or_else(|| Quantity::from_unit(600.0, "mm").unwrap());

    let station_bolt = child(node, "station_bolt")
        .and_then(first_positional_string)
        .unwrap_or_else(|| "M12".into());

    Some(ChassisDef {
        id,
        version,
        description,
        grid_pitch,
        generation,
        width_configs,
        rail_sections,
        section_kinds,
        configurations,
        parts,
        crossmember_spacing_max,
        station_bolt,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = r#"
chassis "mcds-v1" version="0.1.0" {
    description "Modular carbon ladder frame"
    grid_pitch "100 mm"
    generation 1
    width_configs {
        narrow   inner_rail_spacing="900 mm"
        standard inner_rail_spacing="1050 mm"
    }
    rail_sections {
        section "120x60" height="120 mm" width="60 mm" wall="6 mm" material="cfrp/pultruded-ud-t700-epoxy"
    }
    section_kinds {
        front   length_min="900 mm"  length_max="1500 mm" joints="rear"
        central length_min="1600 mm" length_max="2800 mm" joints="front rear"
        rear    length_min="800 mm"  length_max="2200 mm" joints="front"
    }
    configurations {
        config "2/3-length"  sections="front central"
        config "full-length" sections="front central rear"
    }
    parts {
        rail primitive="chassis/mcds-v1/rail-box"
        crossmember primitive="chassis/mcds-v1/crossmember"
        joint_fitting primitive="chassis/mcds-v1/joint-fitting"
    }
    crossmembers spacing_max="600 mm"
    station_bolt "M12"
}
"#;

    #[test]
    fn parses_chassis() {
        let c = parse_chassis("mcds.chassis.kdl", SRC).map_err(|e| format!("{e:?}")).unwrap();
        assert_eq!(c.id, "mcds-v1");
        assert_eq!(c.grid_pitch.to_unit("mm").unwrap(), 100.0);
        assert_eq!(c.width("narrow").unwrap().to_unit("mm").unwrap(), 900.0);
        assert_eq!(c.rail_section("120x60").unwrap().height.to_unit("mm").unwrap(), 120.0);
        assert_eq!(c.section_kinds["central"].joints, vec!["front", "rear"]);
        assert_eq!(c.configuration("2/3-length").unwrap(), &vec!["front".to_string(), "central".to_string()]);
        assert_eq!(c.parts.rail, "chassis/mcds-v1/rail-box");
        assert_eq!(c.station_bolt, "M12");
    }

    #[test]
    fn rejects_unknown_section_in_configuration() {
        let bad = SRC.replace("sections=\"front central\"", "sections=\"front middle\"");
        let e = parse_chassis("x.chassis.kdl", &bad).err().expect("should fail");
        assert!(e.errors.iter().any(|x| x.msg.contains("middle")), "{:?}", e.errors);
    }

    #[test]
    fn rejects_reversed_length_range() {
        let bad = SRC.replace("length_min=\"900 mm\"  length_max=\"1500 mm\"", "length_min=\"1900 mm\" length_max=\"1500 mm\"");
        let e = parse_chassis("x.chassis.kdl", &bad).err().expect("should fail");
        assert!(e.errors.iter().any(|x| x.msg.contains("exceeds")), "{:?}", e.errors);
    }
}
