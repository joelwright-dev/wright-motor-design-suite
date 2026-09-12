//! Manufacturing and assembly output.
//!
//! The brief asks for two things this crate provides. One is making parts: a bill of materials,
//! a cut list and a manufacturing method chosen for the volume being built. The other is the
//! flatpack promise, that anyone who can assemble a shelf can assemble a vehicle, which means
//! step-by-step instructions rather than a drawing and good luck.
//!
//! Everything here is derived from the resolved model. Nothing is authored twice: the fastener
//! on a joint is the fastener in the schedule and the fastener in the instruction, because they
//! are the same value read three ways.

mod assembly_steps;
mod bom;
mod markdown;

pub use assembly_steps::{Step, StepKind, assembly_steps};
pub use bom::{BomLine, CutLine, Fastener, MakePlan, bill_of_materials, cut_list, fasteners};
pub use markdown::{write_markdown, write_text};

use wmds_model::ResolvedAssembly;

/// Everything needed to build one vehicle, in one place.
pub struct BuildPack {
    pub vehicle: String,
    pub version: String,
    /// How many vehicles the costs and method choices assume.
    pub volume: u32,
    pub bom: Vec<BomLine>,
    pub cuts: Vec<CutLine>,
    pub fasteners: Vec<Fastener>,
    pub steps: Vec<Step>,
    /// Things the pack cannot state honestly, listed rather than guessed.
    pub caveats: Vec<String>,
}

impl BuildPack {
    pub fn build(
        asm: &ResolvedAssembly,
        lib: &wmds_model::Library,
        masses: &[wmds_geom::PartMass],
        volume: u32,
    ) -> BuildPack {
        let bom = bill_of_materials(asm, lib, masses, volume);
        let cuts = cut_list(asm, lib);
        let fast = fasteners(asm);
        let steps = assembly_steps(asm);

        let mut caveats = Vec::new();
        if bom.iter().any(|b| b.unit_mass.is_none()) {
            caveats.push(
                "Some parts have no mass, because their material has no density or their \
                 geometry did not build. Their cost and shipping weight are missing."
                    .into(),
            );
        }
        if bom.iter().any(|b| b.plan.is_none()) {
            caveats.push(
                "Some parts declare no manufacturing method that covers this volume, so no \
                 route and no cost could be chosen for them."
                    .into(),
            );
        }
        if fast.iter().any(|f| f.torque.is_none()) {
            caveats.push(
                "Some joints have no torque figure. A kit joint without one cannot be tightened \
                 correctly by the person building the vehicle."
                    .into(),
            );
        }
        let unplaced = asm
            .instances
            .iter()
            .filter(|i| i.placed_by == wmds_model::PlacedBy::Unreached)
            .count();
        if unplaced > 0 {
            caveats.push(format!(
                "{unplaced} part(s) are not connected to anything, so they appear in the bill of \
                 materials but in no assembly step. Nobody could build this as it stands."
            ));
        }
        if !asm.errors.is_empty() {
            caveats.push(format!(
                "The model has {} unresolved error(s); everything here is downstream of them.",
                asm.errors.len()
            ));
        }

        BuildPack {
            vehicle: asm.id.clone(),
            version: asm.version.clone(),
            volume,
            bom,
            cuts,
            fasteners: fast,
            steps,
            caveats,
        }
    }

    /// Total cost of one vehicle at this volume, and the tooling it assumes.
    pub fn cost(&self) -> (Option<f64>, f64) {
        let mut per_vehicle = 0.0;
        let mut known = false;
        let mut tooling = 0.0;
        for line in &self.bom {
            if let Some(plan) = &line.plan {
                if let Some(u) = plan.unit_cost {
                    per_vehicle += u * line.quantity as f64;
                    known = true;
                }
                tooling += plan.fixed_cost.unwrap_or(0.0);
            }
        }
        (known.then_some(per_vehicle), tooling)
    }

    pub fn total_mass(&self) -> f64 {
        self.bom.iter().filter_map(|b| b.total_mass).sum()
    }
}
