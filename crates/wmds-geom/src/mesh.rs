//! Triangle meshes and mass properties of closed meshes.

use std::io::Write;
use std::path::Path;

use crate::Vec3;

/// An indexed triangle mesh in metres. Triangles are counter-clockwise seen from outside.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Mesh {
    pub positions: Vec<Vec3>,
    pub normals: Vec<Vec3>,
    pub triangles: Vec<[u32; 3]>,
}

/// Mass properties of a solid for unit density (multiply by density to get mass and inertia).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MassProps {
    /// m^3
    pub volume: f64,
    /// metres
    pub centroid: Vec3,
    /// Inertia tensor about the centroid, per unit density (m^5). Row-major, symmetric.
    pub inertia: [[f64; 3]; 3],
}

impl Mesh {
    pub fn bounds(&self) -> Option<(Vec3, Vec3)> {
        let first = *self.positions.first()?;
        let mut lo = first;
        let mut hi = first;
        for p in &self.positions {
            for i in 0..3 {
                lo[i] = lo[i].min(p[i]);
                hi[i] = hi[i].max(p[i]);
            }
        }
        Some((lo, hi))
    }

    /// Append another mesh.
    pub fn extend(&mut self, other: &Mesh) {
        let base = self.positions.len() as u32;
        self.positions.extend_from_slice(&other.positions);
        self.normals.extend_from_slice(&other.normals);
        self.triangles.extend(other.triangles.iter().map(|t| [t[0] + base, t[1] + base, t[2] + base]));
    }

    /// Polyhedral mass properties by the divergence theorem (Eberly, "Polyhedral Mass
    /// Properties (Revisited)"). Exact for the polyhedron; the mesh must be closed and
    /// consistently oriented.
    pub fn mass_props(&self) -> MassProps {
        let mult = [1.0 / 6.0, 1.0 / 24.0, 1.0 / 24.0, 1.0 / 24.0, 1.0 / 60.0, 1.0 / 60.0, 1.0 / 60.0, 1.0 / 120.0, 1.0 / 120.0, 1.0 / 120.0];
        let mut intg = [0.0f64; 10];
        for t in &self.triangles {
            let p0 = self.positions[t[0] as usize];
            let p1 = self.positions[t[1] as usize];
            let p2 = self.positions[t[2] as usize];
            let (x0, y0, z0) = (p0[0], p0[1], p0[2]);
            let (x1, y1, z1) = (p1[0], p1[1], p1[2]);
            let (x2, y2, z2) = (p2[0], p2[1], p2[2]);
            let (a1, b1, c1) = (x1 - x0, y1 - y0, z1 - z0);
            let (a2, b2, c2) = (x2 - x0, y2 - y0, z2 - z0);
            let d0 = b1 * c2 - b2 * c1;
            let d1 = a2 * c1 - a1 * c2;
            let d2 = a1 * b2 - a2 * b1;
            let (f1x, f2x, f3x, g0x, g1x, g2x) = subexpr(x0, x1, x2);
            let (f1y, f2y, f3y, g0y, g1y, g2y) = subexpr(y0, y1, y2);
            let (f1z, f2z, f3z, g0z, g1z, g2z) = subexpr(z0, z1, z2);
            intg[0] += d0 * f1x;
            intg[1] += d0 * f2x;
            intg[2] += d1 * f2y;
            intg[3] += d2 * f2z;
            intg[4] += d0 * f3x;
            intg[5] += d1 * f3y;
            intg[6] += d2 * f3z;
            intg[7] += d0 * (y0 * g0x + y1 * g1x + y2 * g2x);
            intg[8] += d1 * (z0 * g0y + z1 * g1y + z2 * g2y);
            intg[9] += d2 * (x0 * g0z + x1 * g1z + x2 * g2z);
            let _ = (f1y, f1z);
        }
        for i in 0..10 {
            intg[i] *= mult[i];
        }
        let volume = intg[0];
        if volume.abs() < 1e-300 {
            return MassProps { volume: 0.0, centroid: [0.0; 3], inertia: [[0.0; 3]; 3] };
        }
        let cx = intg[1] / volume;
        let cy = intg[2] / volume;
        let cz = intg[3] / volume;
        let ixx = intg[5] + intg[6] - volume * (cy * cy + cz * cz);
        let iyy = intg[4] + intg[6] - volume * (cz * cz + cx * cx);
        let izz = intg[4] + intg[5] - volume * (cx * cx + cy * cy);
        let ixy = -(intg[7] - volume * cx * cy);
        let iyz = -(intg[8] - volume * cy * cz);
        let ixz = -(intg[9] - volume * cz * cx);
        MassProps { volume, centroid: [cx, cy, cz], inertia: [[ixx, ixy, ixz], [ixy, iyy, iyz], [ixz, iyz, izz]] }
    }

    /// Write a binary STL (millimetres, the de-facto convention for STL consumers).
    pub fn write_stl_binary(&self, path: &Path) -> std::io::Result<()> {
        let mut f = std::io::BufWriter::new(std::fs::File::create(path)?);
        let mut header = [0u8; 80];
        let tag = b"WMDS binary STL (mm)";
        header[..tag.len()].copy_from_slice(tag);
        f.write_all(&header)?;
        f.write_all(&(self.triangles.len() as u32).to_le_bytes())?;
        for t in &self.triangles {
            let p = [self.positions[t[0] as usize], self.positions[t[1] as usize], self.positions[t[2] as usize]];
            let u = crate::sub(p[1], p[0]);
            let v = crate::sub(p[2], p[0]);
            let n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
            let n = crate::normalize(n).unwrap_or([0.0, 0.0, 1.0]);
            for c in n {
                f.write_all(&(c as f32).to_le_bytes())?;
            }
            for q in p {
                for c in q {
                    f.write_all(&((c * 1000.0) as f32).to_le_bytes())?;
                }
            }
            f.write_all(&0u16.to_le_bytes())?;
        }
        f.flush()
    }
}

#[allow(clippy::type_complexity)]
fn subexpr(w0: f64, w1: f64, w2: f64) -> (f64, f64, f64, f64, f64, f64) {
    let temp0 = w0 + w1;
    let f1 = temp0 + w2;
    let temp1 = w0 * w0;
    let temp2 = temp1 + w1 * temp0;
    let f2 = temp2 + w2 * f1;
    let f3 = w0 * temp1 + w1 * temp2 + w2 * f2;
    let g0 = f2 + w0 * (f1 + w0);
    let g1 = f2 + w1 * (f1 + w1);
    let g2 = f2 + w2 * (f1 + w2);
    (f1, f2, f3, g0, g1, g2)
}

impl MassProps {
    /// Scale by density (kg/m^3) to get mass in kg and inertia in kg m^2.
    pub fn with_density(&self, rho: f64) -> (f64, Vec3, [[f64; 3]; 3]) {
        let mut i = self.inertia;
        for row in &mut i {
            for c in row {
                *c *= rho;
            }
        }
        (self.volume * rho, self.centroid, i)
    }
}

/// An axis-aligned box mesh, used by tests and as a fallback envelope.
pub fn box_mesh(lo: Vec3, hi: Vec3) -> Mesh {
    let p = |x: usize, y: usize, z: usize| -> Vec3 {
        [if x == 0 { lo[0] } else { hi[0] }, if y == 0 { lo[1] } else { hi[1] }, if z == 0 { lo[2] } else { hi[2] }]
    };
    let positions = vec![p(0, 0, 0), p(1, 0, 0), p(1, 1, 0), p(0, 1, 0), p(0, 0, 1), p(1, 0, 1), p(1, 1, 1), p(0, 1, 1)];
    // Outward-facing, counter-clockwise from outside.
    let triangles = vec![
        [0, 2, 1],
        [0, 3, 2], // bottom (z = lo)
        [4, 5, 6],
        [4, 6, 7], // top
        [0, 1, 5],
        [0, 5, 4], // front (y = lo)
        [1, 2, 6],
        [1, 6, 5], // right (x = hi)
        [2, 3, 7],
        [2, 7, 6], // back
        [3, 0, 4],
        [3, 4, 7], // left
    ];
    Mesh { positions, normals: Vec::new(), triangles }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9 * (1.0 + a.abs().max(b.abs()))
    }

    #[test]
    fn unit_cube_mass_props() {
        let m = box_mesh([0.0; 3], [1.0; 3]);
        let mp = m.mass_props();
        assert!(close(mp.volume, 1.0));
        assert!(mp.centroid.iter().all(|c| close(*c, 0.5)));
        assert!(close(mp.inertia[0][0], 1.0 / 6.0));
        assert!(close(mp.inertia[1][1], 1.0 / 6.0));
        assert!(close(mp.inertia[0][1], 0.0));
    }

    #[test]
    fn offset_box_mass_props() {
        // 2 x 3 x 4 box, corner at (10, 20, 30)
        let m = box_mesh([10.0, 20.0, 30.0], [12.0, 23.0, 34.0]);
        let mp = m.mass_props();
        assert!(close(mp.volume, 24.0));
        assert!(close(mp.centroid[0], 11.0) && close(mp.centroid[1], 21.5) && close(mp.centroid[2], 32.0));
        // Ixx = V (b^2 + c^2)/12 with b=3, c=4
        assert!(close(mp.inertia[0][0], 24.0 * (9.0 + 16.0) / 12.0));
        assert!(close(mp.inertia[1][1], 24.0 * (4.0 + 16.0) / 12.0));
        assert!(close(mp.inertia[2][2], 24.0 * (4.0 + 9.0) / 12.0));
        let (mass, _, _) = mp.with_density(7850.0);
        assert!(close(mass, 24.0 * 7850.0));
    }

    #[test]
    fn inverted_mesh_has_negative_volume() {
        let mut m = box_mesh([0.0; 3], [1.0; 3]);
        for t in &mut m.triangles {
            t.swap(1, 2);
        }
        assert!(close(m.mass_props().volume, -1.0));
    }
}
