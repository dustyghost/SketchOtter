//! One shared captured part per logical guide polygon. Connectivity is recorded,
//! positions are deliberately not: deformation follows the guide; topology edits
//! require an explicit new region instead of silently reassigning face numbers.
use super::*;
use std::collections::{BTreeSet, HashMap};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SurfaceFit {
    #[default]
    None,
    Uniform,
    Stretch,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SurfaceArray {
    pub owner: u64,
    pub piece: Piece,
    pub faces: Vec<usize>,
    pub topology: u64,
    pub replace: bool,
    pub scale: f32,
    pub rotation: f32,
    pub offset: f32,
    pub fit: SurfaceFit,
}
impl SurfaceArray {
    pub fn new(owner: u64, piece: Piece, guide: &Mesh, faces: Vec<usize>) -> Result<Self, String> {
        let mut result = Self {
            owner,
            piece,
            faces: Vec::new(),
            topology: 0,
            replace: false,
            scale: 1.,
            rotation: 0.,
            offset: 0.,
            fit: SurfaceFit::None,
        };
        result.select(guide, faces)?;
        Ok(result)
    }
    pub fn select(&mut self, guide: &Mesh, mut faces: Vec<usize>) -> Result<(), String> {
        faces.sort_unstable();
        faces.dedup();
        if faces.is_empty() || faces.len() > MAX_INSTANCES {
            return Err("Select between 1 and 256 guide faces".into());
        }
        let polygons = guide.polygons();
        if faces
            .iter()
            .any(|&i| polygons.get(i).is_none_or(|p| !p.visible))
        {
            return Err("Select visible faces on the guide model".into());
        }
        self.faces = faces;
        self.topology = topology(guide);
        self.validate()
    }
    pub fn validate(&self) -> Result<(), String> {
        self.piece.validate()?;
        if self.owner == 0
            || !matches!(self.piece.source, Source::Asset(_))
            || self.faces.is_empty()
            || self.faces.len() > MAX_INSTANCES
            || self.faces.iter().copied().collect::<BTreeSet<_>>().len() != self.faces.len()
            || !self.scale.is_finite()
            || !(0.001..=1000.).contains(&self.scale)
            || !self.rotation.is_finite()
            || self.rotation.abs() > 3600.
            || !self.offset.is_finite()
            || self.offset.abs() > 1e5
        {
            return Err(
                "Invalid surface array region, captured part, scale, rotation or offset".into(),
            );
        }
        Ok(())
    }
    pub(crate) fn polygons(
        &self,
        guide: &Mesh,
    ) -> Result<Vec<crate::sculpt::edges::Polygon>, String> {
        self.validate()?;
        if topology(guide) != self.topology {
            return Err("Guide topology changed. Edit guide and use selected faces to assign a new region; original faces are shown until then.".into());
        }
        let polygons = guide.polygons();
        self.faces
            .iter()
            .map(|&i| {
                polygons
                    .get(i)
                    .cloned()
                    .ok_or_else(|| "Guide face no longer exists".into())
            })
            .collect()
    }
    pub fn slots(&self, guide: &Mesh, part: &Mesh) -> Result<Vec<Option<Placement>>, String> {
        let polygons = self.polygons(guide)?;
        let mut extent = [0_f32; 2];
        for &v in &part.vertices {
            let v = self.piece.local(v);
            extent[0] = extent[0].max(v.0.abs());
            extent[1] = extent[1].max(v.1.abs());
        }
        polygons.iter().map(|face| {
            if !face.visible { return Ok(None); }
            let points: Vec<_> = face.vertices.iter().map(|&i| guide.vertices[i]).collect();
            let center = points.iter().copied().fold(Vec3::default(), |a,b| a+b) * (1. / points.len() as f32);
            let normal = points.iter().zip(points.iter().cycle().skip(1)).fold(Vec3::default(), |n,(&a,&b)| n+(a-center).cross(b-center));
            if normal.len() < 1e-8 { return Err("A selected guide face is flat or collapsed; restore its area or select a new region".into()); }
            let up = normal.norm();
            let edge = points[1] - points[0];
            let forward = edge - up * edge.dot(up);
            if forward.len() < 1e-6 { return Err("A selected guide edge has zero length".into()); }
            let forward = forward.norm().rotated(up, self.rotation.to_radians());
            let side = up.cross(forward);
            let mut size = [1.; 2];
            if self.fit != SurfaceFit::None {
                if extent.iter().any(|&e| e < 1e-6) { return Err("Fitting needs a part with width and length around its contact point".into()); }
                let mut face_extent = [0_f32; 2];
                for &p in &points {
                    face_extent[0] = face_extent[0].max((p-center).dot(forward).abs());
                    face_extent[1] = face_extent[1].max((p-center).dot(side).abs());
                }
                size = [face_extent[0] / extent[0], face_extent[1] / extent[1]];
                if self.fit == SurfaceFit::Uniform { size = [size[0].min(size[1]); 2]; }
            }
            let normal_scale = if self.fit == SurfaceFit::Uniform { size[0] } else { 1. };
            let transform = Transform { basis: Some(Basis([forward * size[0], side * size[1], up * normal_scale])), origin: center + up * self.offset, forward, up, scale: self.scale };
            transform.validate()?;
            Ok(Some(Placement { piece: self.piece, transform }))
        }).collect()
    }
    /// Suppress only assigned guide polygons. The source mesh is never edited.
    pub fn display_guide(&self, guide: &Mesh) -> Result<Mesh, String> {
        let mut result = guide.clone();
        if !self.replace {
            return Ok(result);
        }
        let hidden = self.replaced_triangles(guide)?;
        result.hidden_faces.resize(guide.triangles.len(), false);
        for i in hidden {
            result.hidden_faces[i] = true;
        }
        Ok(result)
    }
    fn replaced_triangles(&self, guide: &Mesh) -> Result<BTreeSet<usize>, String> {
        let key = |mut t: [usize; 3]| {
            t.sort_unstable();
            t
        };
        let triangles: HashMap<_, _> = guide
            .triangles
            .iter()
            .enumerate()
            .map(|(i, &t)| (key(t), i))
            .collect();
        let mut result = BTreeSet::new();
        for p in self.polygons(guide)? {
            if !p.visible {
                continue;
            }
            let v = p.vertices;
            if v.len() == 3 {
                if let Some(&i) = triangles.get(&key([v[0], v[1], v[2]])) {
                    result.insert(i);
                }
            } else {
                for t in [
                    [v[0], v[1], v[2]],
                    [v[0], v[2], v[3]],
                    [v[0], v[1], v[3]],
                    [v[1], v[2], v[3]],
                ] {
                    if let Some(&i) = triangles.get(&key(t)) {
                        result.insert(i);
                    }
                }
            }
        }
        Ok(result)
    }
    pub fn bake_guide(&self, guide: &Mesh) -> Result<Mesh, String> {
        if !self.replace {
            return Ok(guide.clone());
        }
        let removed = self.replaced_triangles(guide)?;
        let retained: Vec<_> = (0..guide.triangles.len())
            .filter(|i| !removed.contains(i))
            .collect();
        let mut selection = guide.clone();
        selection.set_face_visibility(
            (0..guide.triangles.len())
                .map(|i| !removed.contains(&i))
                .collect(),
        );
        let mut result = selection.visibility_part(true)?;
        // Extraction shows its output by design. Restore the original visibility
        // and group flags for the untouched faces, including hidden regions.
        result.hidden_faces = retained.iter().map(|&i| !guide.face_visible(i)).collect();
        for (out, original) in result.vertex_groups.iter_mut().zip(&guide.vertex_groups) {
            out.visible = original.visible;
        }
        result.validate()?;
        Ok(result)
    }
}
fn topology(mesh: &Mesh) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for n in [mesh.vertices.len(), mesh.triangles.len(), mesh.quads.len()]
        .into_iter()
        .chain(mesh.triangles.iter().flatten().copied())
        .chain(mesh.quads.iter().flatten().copied())
    {
        for b in (n as u64).to_le_bytes() {
            hash = (hash ^ u64::from(b)).wrapping_mul(0x100000001b3);
        }
    }
    hash
}
#[cfg(all(test, not(feature = "focused-retopology")))]
mod tests;
