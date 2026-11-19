//! Component transforms keep the source topology and reflect displacement across symmetry.
use super::*;
#[derive(Clone, Copy)]
pub enum ComponentTransform {
    Move(Vec3),
    Rotate(Vec3, f32),
    Scale(Vec3, f32),
}
impl Mesh {
    /// Slide on the original incident surface, stopping at its boundary. Each
    /// preview starts from the source mesh, so cancellation has no accumulated drift.
    pub(crate) fn slide_components(
        &self,
        selected: &[usize],
        anchor: Vec3,
        displacement: Vec3,
        protect: bool,
        symmetry: &ModelingSymmetry,
    ) -> Result<Mesh, String> {
        let mut out = self.transform_components(
            selected,
            anchor,
            ComponentTransform::Move(displacement),
            protect,
            symmetry,
        )?;
        let hidden = &self.hidden_faces;
        let mut incident = vec![Vec::new(); self.vertices.len()];
        for (i, t) in self.triangles.iter().enumerate() {
            if hidden.get(i).copied().unwrap_or(false) {
                continue;
            }
            for &v in t {
                incident[v].push(i);
            }
        }
        for (v, triangles) in incident.iter().enumerate() {
            if out.vertices[v] == self.vertices[v] {
                continue;
            }
            let target = out.vertices[v];
            let point = triangles
                .iter()
                .map(|&t| {
                    let [a, b, c] = self.triangles[t].map(|i| self.vertices[i]);
                    crate::sculpt::math::closest_triangle(target, a, b, c)
                })
                .min_by(|a, b| (*a - target).len().total_cmp(&(*b - target).len()));
            out.vertices[v] = point.unwrap_or(self.vertices[v]);
        }
        for t in &self.triangles {
            let [a, b, c] = t.map(|v| self.vertices[v]);
            let [x, y, z] = t.map(|v| out.vertices[v]);
            if (b - a).cross(c - a).dot((y - x).cross(z - x)) <= 0. {
                return Err("Slide reaches the next edge; use a shorter movement".into());
            }
        }
        manifold(&out)?;
        symmetry.validate_result(&out)?;
        Ok(out)
    }

    /// Protected source polygons must survive with the same oriented corners.
    pub fn check_model_protection(&self, out: &Mesh, mask: bool) -> Result<(), String> {
        if self.triangles == out.triangles
            && self.quads == out.quads
            && self.loose_edges == out.loose_edges
            && self.loose_vertices == out.loose_vertices
            && self.vertices.len() == out.vertices.len()
        {
            for v in self.protected_model_vertices(mask) {
                if self.vertices[v] != out.vertices[v] {
                    return Err("This edit moves protected vertices".into());
                }
            }
            return Ok(());
        }
        let key = |mesh: &Mesh, p: &Polygon| {
            let points: Vec<_> = p
                .vertices
                .iter()
                .map(|&v| {
                    let p = mesh.vertices[v];
                    [p.0.to_bits(), p.1.to_bits(), p.2.to_bits()]
                })
                .collect();
            (0..points.len())
                .map(|i| {
                    points[i..]
                        .iter()
                        .chain(&points[..i])
                        .copied()
                        .collect::<Vec<_>>()
                })
                .min()
                .unwrap()
        };
        let remaining: BTreeSet<_> = out.polygons().iter().map(|p| key(out, p)).collect();
        for p in self.polygons() {
            if (!p.visible || (mask && p.vertices.iter().any(|&v| self.mask[v] > 0.)))
                && !remaining.contains(&key(self, &p))
            {
                return Err(if !p.visible {
                    "Show hidden adjoining faces before editing"
                } else {
                    "This edit touches masked geometry; clear the mask or turn off Protect mask"
                }
                .into());
            }
        }
        for &v in &self.loose_vertices {
            if mask
                && self.mask[v] > 0.
                && !out
                    .loose_vertices
                    .iter()
                    .any(|&i| out.vertices[i] == self.vertices[v])
            {
                return Err("This edit touches masked isolated vertices; clear the mask or turn off Protect mask".into());
            }
        }
        let wire_key = |mesh: &Mesh, e: [usize; 2]| {
            let mut points = e.map(|v| {
                let p = mesh.vertices[v];
                [p.0.to_bits(), p.1.to_bits(), p.2.to_bits()]
            });
            points.sort_unstable();
            points
        };
        let remaining: BTreeSet<_> = out.loose_edges.iter().map(|&e| wire_key(out, e)).collect();
        let visible: BTreeSet<_> = self.visible_loose_edges().into_iter().collect();
        for &e in &self.loose_edges {
            if (!visible.contains(&e) || (mask && e.iter().any(|&v| self.mask[v] > 0.)))
                && !remaining.contains(&wire_key(self, e))
            {
                return Err(
                    "This edit touches masked or hidden edges; clear their protection first".into(),
                );
            }
        }
        Ok(())
    }

    pub(crate) fn protected_model_vertices(&self, mask: bool) -> BTreeSet<usize> {
        let visible_wires: BTreeSet<_> = self.visible_loose_edges().into_iter().collect();
        self.polygons()
            .into_iter()
            .filter(|p| !p.visible)
            .flat_map(|p| p.vertices)
            .chain(
                self.loose_edges
                    .iter()
                    .filter(|e| !visible_wires.contains(*e))
                    .flatten()
                    .copied(),
            )
            .chain(
                self.mask
                    .iter()
                    .enumerate()
                    .filter_map(|(v, &m)| (mask && m > 0.).then_some(v)),
            )
            .collect()
    }

    pub fn protect_model_vertices(&self, vertices: &[usize], mask: bool) -> Result<(), String> {
        if vertices.is_empty() || vertices.iter().any(|&v| v >= self.vertices.len()) {
            return Err("Select components on the current mesh".into());
        }
        let selected: BTreeSet<_> = vertices.iter().copied().collect();
        if self
            .polygons()
            .iter()
            .any(|p| !p.visible && p.vertices.iter().any(|v| selected.contains(v)))
        {
            return Err("This edit touches hidden faces; show them before editing".into());
        }
        if mask && vertices.iter().any(|&v| self.mask[v] > 0.) {
            return Err(
                "This edit touches masked vertices; clear the mask or turn off Protect mask".into(),
            );
        }
        Ok(())
    }
    /// Use the same reference-side vertices for the gizmo and the symmetric edit.
    pub(crate) fn component_transform_frame(
        &self,
        selected: &[usize],
        anchor: Vec3,
        symmetry: &ModelingSymmetry,
    ) -> (Vec<usize>, Vec3) {
        let all: BTreeSet<_> = selected
            .iter()
            .flat_map(|&v| symmetry.vertex_copies(v))
            .collect();
        let mut seeds = Vec::new();
        let mut visited = BTreeSet::new();
        for &v in &all {
            if visited.contains(&v) {
                continue;
            }
            let copies = symmetry.vertex_copies(v);
            let seed = *copies
                .iter()
                .min_by(|&&a, &&b| {
                    (self.vertices[a] - anchor)
                        .len()
                        .total_cmp(&(self.vertices[b] - anchor).len())
                })
                .unwrap();
            visited.extend(copies);
            seeds.push(seed);
        }
        let pivot = seeds
            .iter()
            .fold(Vec3::default(), |a, &v| a + self.vertices[v])
            * (1. / seeds.len().max(1) as f32);
        (seeds, pivot)
    }
    pub fn transform_components(
        &self,
        selected: &[usize],
        anchor: Vec3,
        transform: ComponentTransform,
        protect: bool,
        symmetry: &ModelingSymmetry,
    ) -> Result<Mesh, String> {
        if selected.iter().any(|&v| v >= self.vertices.len()) {
            return Err("Select components on the current mesh".into());
        }
        let blocked = self.protected_model_vertices(protect);
        let selected: Vec<_> = selected
            .iter()
            .copied()
            .filter(|&v| {
                symmetry
                    .vertex_copies(v)
                    .iter()
                    .all(|v| !blocked.contains(v))
            })
            .collect();
        let (seeds, pivot) = self.component_transform_frame(&selected, anchor, symmetry);
        let mut out = self.clone();
        for seed in seeds {
            let p = self.vertices[seed];
            let delta = match transform {
                ComponentTransform::Move(v) => v,
                ComponentTransform::Rotate(axis, angle) => {
                    (p - pivot).rotated(axis.norm(), angle) - (p - pivot)
                }
                ComponentTransform::Scale(axis, factor) => {
                    if !factor.is_finite() || factor <= 0. {
                        return Err("Scale must be greater than zero".into());
                    }
                    let offset = p - pivot;
                    if axis.len() < 1e-8 {
                        offset * (factor - 1.)
                    } else {
                        axis.norm() * (offset.dot(axis.norm()) * (factor - 1.))
                    }
                }
            };
            // Stabilizers on mirror planes contribute more than once; average their
            // displacements to keep shared seam vertices on the symmetry plane.
            let mut moves = BTreeMap::<usize, (Vec3, usize)>::new();
            for (mapping, basis) in symmetry.vertices.iter().zip(&symmetry.bases) {
                let value = basis[0] * delta.0 + basis[1] * delta.1 + basis[2] * delta.2;
                let e = moves.entry(mapping[seed]).or_default();
                e.0 = e.0 + value;
                e.1 += 1;
            }
            for (v, (delta, n)) in moves {
                out.vertices[v] = self.vertices[v] + delta * (1. / n as f32);
            }
        }
        manifold(&out)?;
        symmetry.validate_result(&out)?;
        Ok(out)
    }
}
