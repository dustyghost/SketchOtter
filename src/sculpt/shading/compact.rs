//! Fixed-topology release with four-byte corner parents and one sum per output fan.
use super::*;

fn compact_root(parents: &mut [u32], mut i: u32) -> u32 {
    while parents[i as usize] != i {
        let next = parents[parents[i as usize] as usize];
        parents[i as usize] = next;
        i = next;
    }
    i
}

impl SurfaceShadingTopology {
    pub(crate) fn compact_shade(
        &self,
        mesh: &Mesh,
        mut stage: impl FnMut(&'static str),
    ) -> SurfaceShading {
        let Some(corners) = mesh
            .triangles
            .len()
            .checked_mul(3)
            .and_then(|v| u32::try_from(v).ok())
        else {
            return self.shade(mesh);
        };
        let mut parents: Vec<u32> = (0..corners).collect();
        stage("parents");
        let normals = face_normals(mesh);
        stage("face_normals");
        for &[a, x] in &self.paired_corners {
            if normals[a / 3].dot(normals[x / 3]) > 0.5 {
                let b = a / 3 * 3 + (a % 3 + 1) % 3;
                let y = x / 3 * 3 + (x % 3 + 1) % 3;
                for (i, j) in [(a, y), (b, x)] {
                    let i = compact_root(&mut parents, i as u32);
                    let j = compact_root(&mut parents, j as u32);
                    parents[i.max(j) as usize] = i.min(j);
                }
            }
        }
        stage("edge_threshold_and_fan_joins");
        let mut out = SurfaceShading {
            vertices: Vec::with_capacity(mesh.vertices.len()),
            triangles: Vec::with_capacity(mesh.triangles.len()),
        };
        for (face, t) in mesh.triangles.iter().enumerate() {
            let weights = self.angles.get(mesh, face, t);
            let mut triangle = [0; 3];
            for corner in 0..3 {
                let id = face * 3 + corner;
                let parent = parents[id] as usize;
                // Roots use the earliest source corner. Every non-root's
                // immediate parent already holds its final emitted index.
                let fan = if parent == id {
                    let fan = out.vertices.len();
                    out.vertices.push((t[corner], Vec3::default()));
                    fan
                } else {
                    parents[parent] as usize
                };
                parents[id] = fan as u32;
                out.vertices[fan].1 = out.vertices[fan].1 + normals[face] * weights[corner];
                triangle[corner] = fan;
            }
            out.triangles.push(triangle);
        }
        stage("corner_weights_accumulation_and_remapping");
        for (_, normal) in &mut out.vertices {
            *normal = normal.norm();
        }
        stage("normalise_fans");
        out
    }
}
