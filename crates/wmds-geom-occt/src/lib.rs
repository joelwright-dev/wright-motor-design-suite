//! OpenCASCADE implementation of [`wmds_geom::GeomKernel`].
//!
//! The kernel works in millimetres internally so that STEP files come out in the units every
//! other CAD tool expects. The trait speaks metres; conversion happens at this boundary only.

use std::path::Path;
use std::sync::Arc;

use glam::DVec3;
use opencascade::primitives::Shape;
use wmds_geom::{GeomError, GeomKernel, Mesh, Result, Vec3};

const M_TO_MM: f64 = 1000.0;

#[derive(Default)]
pub struct OcctKernel;

#[derive(Clone)]
pub struct OcctSolid(pub Arc<Shape>);

fn mm(v: Vec3) -> DVec3 {
    DVec3::new(v[0] * M_TO_MM, v[1] * M_TO_MM, v[2] * M_TO_MM)
}

fn dir(v: Vec3) -> DVec3 {
    DVec3::new(v[0], v[1], v[2])
}

impl GeomKernel for OcctKernel {
    type Solid = OcctSolid;

    fn make_box(&self, size: Vec3, centre: Vec3) -> Result<OcctSolid> {
        let half = [size[0] / 2.0, size[1] / 2.0, size[2] / 2.0];
        let lo = mm(wmds_geom::sub(centre, half));
        let hi = mm(wmds_geom::add(centre, half));
        Ok(OcctSolid(Arc::new(Shape::box_from_corners(lo, hi))))
    }

    fn cylinder_between(&self, a: Vec3, b: Vec3, r: f64) -> Result<OcctSolid> {
        if r <= 0.0 {
            return Err(GeomError::Kernel(format!(
                "cylinder radius {r} must be positive"
            )));
        }
        let pa = mm(a);
        let pb = mm(b);
        if (pb - pa).length() == 0.0 {
            return Err(GeomError::Kernel("cylinder endpoints coincide".into()));
        }
        Ok(OcctSolid(Arc::new(Shape::cylinder_from_points(pa, pb, r * M_TO_MM))))
    }

    fn union(&self, a: &OcctSolid, b: &OcctSolid) -> Result<OcctSolid> {
        Ok(OcctSolid(Arc::new(a.0.union(&b.0).shape)))
    }

    fn subtract(&self, a: &OcctSolid, b: &OcctSolid) -> Result<OcctSolid> {
        Ok(OcctSolid(Arc::new(a.0.subtract(&b.0).shape)))
    }

    fn intersect(&self, a: &OcctSolid, b: &OcctSolid) -> Result<OcctSolid> {
        Ok(OcctSolid(Arc::new(a.0.intersect(&b.0).shape)))
    }

    fn translated(&self, s: &OcctSolid, offset: Vec3) -> Result<OcctSolid> {
        Ok(OcctSolid(Arc::new(s.0.translated(mm(offset)))))
    }

    fn mirrored(&self, s: &OcctSolid, origin: Vec3, normal: Vec3) -> Result<OcctSolid> {
        Ok(OcctSolid(Arc::new(s.0.mirrored(mm(origin), dir(normal)))))
    }

    fn rotated(&self, s: &OcctSolid, axis: Vec3, angle: f64) -> Result<OcctSolid> {
        Ok(OcctSolid(Arc::new(s.0.rotated(dir(axis), angle))))
    }

    fn tessellate(&self, s: &OcctSolid, tolerance: f64) -> Result<Mesh> {
        let m =
            s.0.mesh_with_tolerance(tolerance * M_TO_MM)
                .map_err(|e| GeomError::Kernel(format!("tessellation failed: {e}")))?;
        let positions: Vec<Vec3> = m
            .vertices
            .iter()
            .map(|v| [v.x / M_TO_MM, v.y / M_TO_MM, v.z / M_TO_MM])
            .collect();
        let normals: Vec<Vec3> = m.normals.iter().map(|n| [n.x, n.y, n.z]).collect();
        let triangles: Vec<[u32; 3]> = m
            .indices
            .chunks_exact(3)
            .map(|c| [c[0] as u32, c[1] as u32, c[2] as u32])
            .collect();
        Ok(Mesh {
            positions,
            normals,
            triangles,
        })
    }

    fn write_step(&self, s: &OcctSolid, path: &Path) -> Result<()> {
        s.0.write_step(path)
            .map_err(|e| GeomError::Io(format!("STEP write failed: {e}")))
    }

    fn read_step(&self, path: &Path) -> Result<OcctSolid> {
        Shape::read_step(path)
            .map(|s| OcctSolid(Arc::new(s)))
            .map_err(|e| GeomError::Io(format!("STEP read failed: {e}")))
    }

    fn write_stl(&self, s: &OcctSolid, path: &Path) -> Result<()> {
        s.0.write_stl(path)
            .map_err(|e| GeomError::Io(format!("STL write failed: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, rel: f64) -> bool {
        (a - b).abs() <= rel * b.abs().max(1e-12)
    }

    #[test]
    fn box_volume_and_centroid() {
        let k = OcctKernel;
        let b = k.make_box([0.2, 0.3, 0.4], [1.0, 2.0, 3.0]).unwrap();
        let mp = k.mass_props(&b).unwrap();
        assert!(close(mp.volume, 0.024, 1e-6), "volume {}", mp.volume);
        assert!(
            close(mp.centroid[0], 1.0, 1e-6)
                && close(mp.centroid[1], 2.0, 1e-6)
                && close(mp.centroid[2], 3.0, 1e-6)
        );
    }

    #[test]
    fn tube_volume_matches_analytic() {
        let k = OcctKernel;
        // od 28 mm, wall 2.5 mm, length 300 mm
        let t = k
            .tube_between([0.0; 3], [0.3, 0.0, 0.0], 0.028, 0.0025)
            .unwrap();
        let mp = k.mass_props(&t).unwrap();
        let ro: f64 = 0.014;
        let ri: f64 = 0.0115;
        let expected = std::f64::consts::PI * (ro * ro - ri * ri) * 0.3;
        // Tessellated circles underestimate area slightly; 1 % is a reasonable bound.
        assert!(
            close(mp.volume, expected, 0.01),
            "volume {} vs {}",
            mp.volume,
            expected
        );
        assert!(close(mp.centroid[0], 0.15, 1e-3));
    }

    #[test]
    fn union_and_step_roundtrip() {
        let k = OcctKernel;
        let a = k.make_box([0.1, 0.1, 0.1], [0.0; 3]).unwrap();
        let b = k.make_box([0.1, 0.1, 0.1], [0.05, 0.0, 0.0]).unwrap();
        let u = k.union(&a, &b).unwrap();
        let v = k.mass_props(&u).unwrap().volume;
        assert!(close(v, 0.0015, 1e-6), "volume {v}");
        let dir = std::env::temp_dir().join("wmds-occt-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("union.step");
        k.write_step(&u, &path).unwrap();
        let back = k.read_step(&path).unwrap();
        let v2 = k.mass_props(&back).unwrap().volume;
        assert!(close(v2, 0.0015, 1e-4), "roundtrip volume {v2}");
    }
}
