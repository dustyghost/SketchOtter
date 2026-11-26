//! Non-destructive polygon-group selection and perimeter reconstruction.
use super::{Mesh, Vec3, edges::edge};
use std::collections::{BTreeMap, BTreeSet};

/// A selected group copied onto its own open surface. Coordinates and
/// attributes are retained; perimeter cleanup and thickening are not applied.
pub struct PanelRegion {
    pub group: usize,
    pub surface: Mesh,
    /// Directed boundary cycles, including holes and disconnected islands.
    pub perimeters: Vec<Vec<usize>>,
}

/// Widths are in model units. The gap is the total separation between two
/// adjacent panels, so each panel retreats by half the requested distance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PanelBorder {
    pub gap: f32,
    pub width: f32,
    pub loops: u32,
    pub smoothing: f32,
    /// Exponent of cross-border spacing: >1 concentrates support near the bevel.
    pub spacing: f32,
    /// Turns at or above this angle stay pinned; crease endpoints always stay pinned.
    pub corner_angle: f32,
}
impl Default for PanelBorder {
    fn default() -> Self {
        Self {
            gap: 0.04,
            width: 0.04,
            loops: 2,
            smoothing: 0.,
            spacing: 1.,
            corner_angle: 75.,
        }
    }
}
impl PanelBorder {
    fn validate(self) -> Result<(), String> {
        if !self.gap.is_finite()
            || !(0. ..=100.).contains(&self.gap)
            || !self.width.is_finite()
            || !(0.00001..=100.).contains(&self.width)
            || !(0. ..=1.).contains(&self.smoothing)
            || !(1..=8).contains(&self.loops)
            || !(0.25..=4.).contains(&self.spacing)
            || !(15. ..=150.).contains(&self.corner_angle)
        {
            Err("Use positive border width, 1–8 loops, nonnegative gap, spacing 0.25–4, smoothing 0–1 and corner angle 15–150°".into())
        } else {
            Ok(())
        }
    }
}

impl PanelRegion {
    /// Reconstruct the perimeter by moving its original vertices to an inset
    /// ring and welding continuous quad strips onto it. Faces incident to that
    /// ring change shape; all other interior vertices and connectivity stay exact.
    /// New strips intentionally have unmapped UVs until a wall UV layout is made.
    pub fn rebuild_border(
        &self,
        options: PanelBorder,
        cancelled: &impl Fn() -> bool,
    ) -> Result<Mesh, String> {
        options.validate()?;
        check_cancel(cancelled)?;
        let source = &self.surface;
        let edges: usize = self.perimeters.iter().map(Vec::len).sum();
        let added = edges
            .checked_mul(options.loops as usize)
            .ok_or("Panel border exceeds the mesh size limit")?;
        if source.vertices.len() + added > super::max_vertices()
            || source.triangles.len() + added * 2 > super::max_triangles()
        {
            return Err("Panel border exceeds the mesh size limit".into());
        }
        let normals = source.angle_weighted_normals();
        let mut out = source.clone();
        let surface = super::projection::Surface::new(source, cancelled)?;
        let mut budget = 10_000_000usize;
        let mut projected = |p: Vec3, origin: Vec3, strict: bool| -> Result<Vec3, String> {
            let distance = (p - origin).len().max(0.0001);
            let (_, q, _) = surface.nearest(p, distance, None, &mut budget)?.ok_or(
                "Panel outline left the source surface; reduce gap, border width or smoothing",
            )?;
            if strict && (q - p).len() > distance * 0.5 {
                return Err("Panel border leaves or crosses a narrow source region; reduce gap, width or smoothing".into());
            }
            Ok(q)
        };
        let crease_vertices: BTreeSet<_> = source.creases.iter().flatten().copied().collect();
        let mut rings = BTreeMap::new();
        for cycle in &self.perimeters {
            // Simultaneous, tangential relaxation also redistributes uneven edge
            // lengths. Only this boundary ring moves; deliberately sharp corners
            // and marked crease endpoints stay fixed.
            let mut outline: Vec<_> = cycle.iter().map(|&v| source.vertices[v]).collect();
            let pinned: Vec<_> = (0..cycle.len())
                .map(|i| {
                    let p = outline[i];
                    let a = outline[(i + cycle.len() - 1) % cycle.len()];
                    let b = outline[(i + 1) % cycle.len()];
                    crease_vertices.contains(&cycle[i])
                        || (p - a).norm().dot((b - p).norm())
                            <= options.corner_angle.to_radians().cos()
                })
                .collect();
            for _ in 0..4 {
                check_cancel(cancelled)?;
                let old = outline.clone();
                for i in 0..cycle.len() {
                    if !pinned[i] && options.smoothing > 0. {
                        let p = old[i];
                        let mean = (old[(i + cycle.len() - 1) % cycle.len()]
                            + old[(i + 1) % cycle.len()])
                            * 0.5;
                        outline[i] = projected(
                            p + (mean - p) * (options.smoothing * 0.5),
                            source.vertices[cycle[i]],
                            false,
                        )?;
                    }
                }
            }
            for (i, &v) in cycle.iter().enumerate() {
                if cancelled() {
                    return Err("Panel preview cancelled".into());
                }
                let p = outline[i];
                let a = outline[(i + cycle.len() - 1) % cycle.len()];
                let b = outline[(i + 1) % cycle.len()];
                let incoming = (p - a).norm();
                let outgoing = (b - p).norm();
                let left = normals[v].cross(incoming);
                let right = normals[v].cross(outgoing);
                let denominator = 1. + left.dot(right);
                if denominator < 0.1 {
                    return Err("A panel corner is too sharp for this border; reduce the width or simplify the outline".into());
                }
                let inward = (left + right) * (1. / denominator);
                let outer = p + inward * (options.gap * 0.5);
                out.vertices[v] = projected(
                    p + inward * (options.gap * 0.5 + options.width),
                    source.vertices[v],
                    true,
                )?;
                let mut ring = Vec::with_capacity(options.loops as usize + 1);
                for layer in 0..options.loops {
                    let position = projected(
                        outer
                            + inward
                                * (options.width
                                    * (layer as f32 / options.loops as f32).powf(options.spacing)),
                        source.vertices[v],
                        true,
                    )?;
                    ring.push(out.append_blend(source, &[(v, 1.)], position));
                }
                ring.push(v);
                rings.insert(v, ring);
            }
        }
        let first_new = out.triangles.len();
        for cycle in &self.perimeters {
            for (i, &a) in cycle.iter().enumerate() {
                if cancelled() {
                    return Err("Panel preview cancelled".into());
                }
                let b = cycle[(i + 1) % cycle.len()];
                for layer in 0..options.loops as usize {
                    let aa = rings[&a][layer];
                    let bb = rings[&b][layer];
                    let cc = rings[&b][layer + 1];
                    let dd = rings[&a][layer + 1];
                    out.quads.push([aa, bb, cc, dd]);
                    out.triangles.extend([[aa, bb, cc], [aa, cc, dd]]);
                    if !out.polygroups.is_empty() {
                        out.polygroups.extend([self.group; 2]);
                    }
                    if !out.hidden_faces.is_empty() {
                        out.hidden_faces.extend([false; 2]);
                    }
                    if source.uv.current(source) {
                        out.uv.corners.extend([[[0.; 2]; 3]; 2]);
                        if !out.uv.masks.is_empty() {
                            out.uv.masks.extend([[0.; 3]; 2]);
                        }
                        if out.uv.removed.is_empty() {
                            out.uv.removed.resize(first_new, false);
                        }
                        out.uv.removed.extend([true; 2]);
                    }
                }
            }
        }
        if source.uv.current(source) {
            out.uv.topology = out.triangles.clone();
        }
        let seams: BTreeSet<_> = source.shading_edges().iter().copied().collect();
        out.set_shading_seams(
            seams
                .iter()
                .copied()
                .chain(self.perimeters.iter().flat_map(|cycle| {
                    cycle
                        .iter()
                        .copied()
                        .zip(cycle.iter().copied().cycle().skip(1))
                        .filter_map(|(a, b)| {
                            seams
                                .contains(&edge(a, b))
                                .then_some(edge(rings[&a][0], rings[&b][0]))
                        })
                }))
                .collect(),
        );
        for (old, new) in source.triangles.iter().zip(&out.triangles) {
            let [a, b, c] = *old;
            let before = (source.vertices[b] - source.vertices[a])
                .cross(source.vertices[c] - source.vertices[a]);
            let [a, b, c] = *new;
            let after =
                (out.vertices[b] - out.vertices[a]).cross(out.vertices[c] - out.vertices[a]);
            if before.dot(after) <= 0. {
                return Err("Panel border pinches or folds an interior face; reduce gap, width, or smoothing".into());
            }
        }
        out.validate()?;
        super::modeling::manifold(&out)?;
        super::fusion::validate_surface(&out)?;
        super::fusion::validate_panel_faces(&out, 0, cancelled).map_err(|e| {
            format!("Panel border crosses another surface; reduce gap, width or smoothing. {e}")
        })?;
        Ok(out)
    }
}

impl Mesh {
    /// Prepare independent panel source surfaces without changing the sculpt.
    /// Each group may contain holes and disconnected islands, but its borders
    /// must be closed and nonbranching. The returned cycles index `surface`.
    pub fn panel_regions(
        &self,
        groups: &[usize],
        protect_mask: bool,
        cancelled: &impl Fn() -> bool,
    ) -> Result<Vec<PanelRegion>, String> {
        check_cancel(cancelled)?;
        self.validate()?;
        if !self.uv.corners.is_empty() && !self.uv.current(self) {
            return Err("Restore or clear inactive UVs before generating panels".into());
        }
        if cancelled() {
            return Err("Panel preview cancelled".into());
        }
        let requested: BTreeSet<_> = groups.iter().copied().collect();
        if requested.is_empty() || requested.contains(&0) {
            return Err("Select one or more polygon groups for panels".into());
        }
        if requested.len() > 64 {
            return Err("Generate at most 64 panels at a time".into());
        }
        let polygons = self.polygons();
        let triangle_indices = super::uv::polygon_triangles(self);
        let mut result = Vec::new();
        for &group in &requested {
            if cancelled() {
                return Err("Panel preview cancelled".into());
            }
            let selected: Vec<_> = polygons
                .iter()
                .enumerate()
                .filter_map(|(i, p)| (p.group == group).then_some(i))
                .collect();
            if selected.is_empty() {
                return Err(format!("Polygon group {group} has no faces"));
            }
            if selected.len() > super::max_triangles() {
                return Err("Panel surface exceeds the mesh size limit".into());
            }
            if selected.iter().any(|&i| !polygons[i].visible) {
                return Err(format!(
                    "Show all faces of polygon group {group} before generating panels"
                ));
            }
            let touched: Vec<_> = selected
                .iter()
                .flat_map(|&i| polygons[i].vertices.iter().copied())
                .collect();
            self.protect_model_vertices(&touched, protect_mask)?;

            let mut boundary = BTreeMap::new();
            let mut incidence = BTreeMap::<_, Vec<_>>::new();
            for &i in &selected {
                if i % 256 == 0 && cancelled() {
                    return Err("Panel preview cancelled".into());
                }
                for (a, b) in polygons[i].edges() {
                    let uses = incidence.entry(edge(a, b)).or_default();
                    uses.push(a);
                    if uses.len() > 2 || uses.len() == 2 && uses[0] == uses[1] {
                        return Err(
                            "Panel group requires consistently oriented manifold faces".into()
                        );
                    }
                    if boundary.remove(&edge(a, b)).is_none() {
                        boundary.insert(edge(a, b), (a, b));
                    }
                }
            }
            let mut next = BTreeMap::new();
            let mut incoming = BTreeSet::new();
            for &(a, b) in boundary.values() {
                if next.insert(a, b).is_some() || !incoming.insert(b) {
                    return Err(format!(
                        "Polygon group {group} has touching or branching borders"
                    ));
                }
            }
            if next.is_empty()
                || next.len() != incoming.len()
                || next.keys().any(|v| !incoming.contains(v))
            {
                return Err(format!(
                    "Polygon group {group} needs closed, nonbranching perimeters"
                ));
            }
            let mut perimeters = Vec::new();
            while let Some((&start, _)) = next.first_key_value() {
                let mut cycle = Vec::new();
                let mut at = start;
                loop {
                    cycle.push(at);
                    at = next
                        .remove(&at)
                        .ok_or("Panel perimeter is not a closed loop")?;
                    if at == start {
                        break;
                    }
                }
                if cycle.len() < 3 {
                    return Err("Panel perimeter has fewer than three vertices".into());
                }
                perimeters.push(cycle);
            }

            let mut surface = Mesh::default();
            let mut indices = BTreeMap::new();
            let mut old_vertices = Vec::new();
            let mut old_faces = Vec::new();
            for &i in &selected {
                if cancelled() {
                    return Err("Panel preview cancelled".into());
                }
                for &v in &polygons[i].vertices {
                    indices.entry(v).or_insert_with(|| {
                        let index = surface.vertices.len();
                        surface.vertices.push(self.vertices[v]);
                        surface.mask.push(self.mask[v]);
                        old_vertices.push(v);
                        index
                    });
                }
                if polygons[i].vertices.len() == 4 {
                    surface
                        .quads
                        .push(std::array::from_fn(|k| indices[&polygons[i].vertices[k]]));
                }
                for &f in &triangle_indices[i] {
                    surface
                        .triangles
                        .push(self.triangles[f].map(|v| indices[&v]));
                    old_faces.push(f);
                }
            }
            self.reindex_groups(&mut surface, &old_vertices, &old_faces);
            surface.loose_edges.clear();
            surface.loose_vertices.clear();
            surface.uv.textures = self.uv.textures.clone();
            {
                surface.uv.seams = self
                    .uv
                    .seams
                    .iter()
                    .filter_map(|&[a, b]| Some(edge(*indices.get(&a)?, *indices.get(&b)?)))
                    .collect();
                surface.uv.seams.sort_unstable();
                surface.uv.seams.dedup();
            }
            surface.set_shading_seams(
                self.shading_edges()
                    .iter()
                    .filter_map(|&[a, b]| Some(edge(*indices.get(&a)?, *indices.get(&b)?)))
                    .collect(),
            );
            surface.validate()?;
            super::modeling::manifold(&surface)?;
            let perimeters = perimeters
                .into_iter()
                .map(|cycle| cycle.into_iter().map(|v| indices[&v]).collect())
                .collect();
            result.push(PanelRegion {
                group,
                surface,
                perimeters,
            });
        }
        Ok(result)
    }
}

/// Profile shared with the component bevel tool; width and depth are equal.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PanelProfile {
    Flat,
    #[default]
    Rounded,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PanelOptions {
    /// Total thickness, centred at `offset` along the source normal.
    pub thickness: f32,
    pub offset: f32,
    pub bevel: f32,
    pub profile: PanelProfile,
    pub segments: u32,
    /// Extra support loops through the straight wall, independent of the surface border.
    pub wall_loops: u32,
    pub border: PanelBorder,
}
impl Default for PanelOptions {
    fn default() -> Self {
        Self {
            thickness: 0.08,
            offset: 0.05,
            bevel: 0.015,
            profile: PanelProfile::Rounded,
            segments: 3,
            wall_loops: 1,
            border: PanelBorder::default(),
        }
    }
}
impl PanelOptions {
    pub fn validate(self) -> Result<(), String> {
        self.border.validate()?;
        if !self.thickness.is_finite()
            || !(0.00001..=100.).contains(&self.thickness)
            || !self.offset.is_finite()
            || !(-100. ..=100.).contains(&self.offset)
            || !self.bevel.is_finite()
            || self.bevel < 0.
            || self.bevel * 2. >= self.thickness
            || !(1..=8).contains(&self.segments)
            || self.wall_loops > 8
        {
            return Err("Use positive thickness, bevel smaller than half the thickness, 1–8 bevel segments and 0–8 wall loops".into());
        }
        Ok(())
    }
}

pub struct GeneratedPanel {
    pub group: usize,
    pub mesh: Mesh,
}
pub struct PanelSelection {
    pub display: Mesh,
    pub outlines: Vec<Vec<Vec3>>,
    pub count: usize,
}
pub struct PanelPreview {
    pub panels: Vec<GeneratedPanel>,
    /// Original outlines for comparison with the reconstructed result.
    pub outlines: Vec<Vec<Vec3>>,
    pub display: Mesh,
}

/// Conservative working budgets include all selected groups, not each one separately.
const MAX_PANEL_INPUT: usize = 250_000;
const MAX_PANEL_OUTPUT: usize = 500_000;
fn check_cancel(cancel: &impl Fn() -> bool) -> Result<(), String> {
    if cancel() {
        Err("Panel preview cancelled".into())
    } else {
        Ok(())
    }
}
impl Mesh {
    fn panel_group_ids(
        &self,
        groups: &[usize],
        symmetry: &super::Symmetry,
        cancel: &impl Fn() -> bool,
    ) -> Result<Vec<usize>, String> {
        check_cancel(cancel)?;
        if self.triangles.len() > MAX_PANEL_INPUT {
            return Err("Panel previews support up to 250,000 source triangles; use a lower level or split the source".into());
        }
        self.validate()?;
        let mut groups: BTreeSet<_> = groups.iter().copied().collect();
        if symmetry.enabled {
            let mapping = super::ModelingSymmetry::new(self, symmetry)?;
            check_cancel(cancel)?;
            let polygons = self.polygons();
            let selected: BTreeSet<_> = polygons
                .iter()
                .enumerate()
                .filter(|(_, p)| groups.contains(&p.group))
                .flat_map(|(i, _)| mapping.face_copies(i))
                .collect();
            groups.extend(selected.iter().map(|&i| polygons[i].group));
            if groups.contains(&0)
                || polygons
                    .iter()
                    .enumerate()
                    .any(|(i, p)| groups.contains(&p.group) && !selected.contains(&i))
            {
                return Err("Symmetry needs matching whole polygon groups on every enabled side; adjust the groups or disable symmetry".into());
            }
        }
        Ok(groups.into_iter().collect())
    }
    /// A private, open-surface highlight before shell generation. Source positions
    /// and outlines are unchanged; Apply stays disabled for this result.
    pub fn preview_panel_selection(
        &self,
        groups: &[usize],
        protect: bool,
        symmetry: &super::Symmetry,
        cancel: &impl Fn() -> bool,
    ) -> Result<PanelSelection, String> {
        let groups = self.panel_group_ids(groups, symmetry, cancel)?;
        let regions = self.panel_regions(&groups, protect, cancel)?;
        let count = regions.len();
        let mut outlines = Vec::new();
        let mut display = None;
        for region in regions {
            check_cancel(cancel)?;
            outlines.extend(
                region
                    .perimeters
                    .iter()
                    .map(|r| r.iter().map(|&v| region.surface.vertices[v]).collect()),
            );
            if let Some(out) = &mut display {
                append_panel(out, &region.surface);
            } else {
                display = Some(region.surface);
            }
        }
        Ok(PanelSelection {
            display: display.ok_or("Select a polygon group")?,
            outlines,
            count,
        })
    }
    pub fn generate_panels(
        &self,
        groups: &[usize],
        options: PanelOptions,
        protect_mask: bool,
        symmetry: &super::Symmetry,
        cancel: &impl Fn() -> bool,
    ) -> Result<PanelPreview, String> {
        options.validate()?;
        check_cancel(cancel)?;
        if self.triangles.len() > MAX_PANEL_INPUT {
            return Err("Panel previews support up to 250,000 source triangles; use a lower subdivision level or split the source first".into());
        }
        self.validate()?;
        let groups = self.panel_group_ids(groups, symmetry, cancel)?;
        let regions = self.panel_regions(&groups, protect_mask, cancel)?;
        let wall_rings = if options.bevel > 0. {
            2 * options.segments as usize
        } else {
            0
        } + options.wall_loops as usize;
        let (mut vertices, mut triangles) = (0usize, 0usize);
        for r in &regions {
            let boundary: usize = r.perimeters.iter().map(Vec::len).sum();
            vertices += 2 * (r.surface.vertices.len() + boundary * options.border.loops as usize)
                + boundary * wall_rings;
            triangles += 2
                * (r.surface.triangles.len() + 2 * boundary * options.border.loops as usize)
                + 2 * boundary * (wall_rings + 1);
        }
        if vertices > super::max_vertices().min(MAX_PANEL_OUTPUT)
            || triangles > super::max_triangles().min(MAX_PANEL_OUTPUT)
        {
            return Err("Panel preview exceeds the mesh/500,000 triangle working budget; select fewer groups or reduce border, bevel or wall loops".into());
        }
        let mut panels = Vec::new();
        let mut outlines = Vec::new();
        for region in regions {
            check_cancel(cancel)?;
            outlines.extend(
                region
                    .perimeters
                    .iter()
                    .map(|ring| ring.iter().map(|&v| region.surface.vertices[v]).collect()),
            );
            let mut mesh = region
                .generate(options, cancel)
                .map_err(|e| format!("Group {}: {e}", region.group))?;
            // An explicitly unmapped result also prevents the document's legacy
            // empty-UV fallback from attaching unrelated source seam indices.
            if self.uv.corners.is_empty() && !self.uv.is_empty() {
                mesh.uv.topology = mesh.triangles.clone();
                mesh.uv.corners = vec![[[0.; 2]; 3]; mesh.triangles.len()];
                mesh.uv.removed = vec![true; mesh.triangles.len()];
            }
            panels.push(GeneratedPanel {
                group: region.group,
                mesh,
            });
        }
        // One shared registry and texture binding, with independent vertices for each panel.
        let mut display = panels[0].mesh.clone();
        for panel in &panels[1..] {
            check_cancel(cancel)?;
            append_panel(&mut display, &panel.mesh);
        }
        display.validate_closed()?;
        super::fusion::validate_panel_faces(&display, 0, cancel)
            .map_err(|e| format!("Panels overlap or self-intersect; increase gap or reduce thickness/offset/bevel. {e}"))?;
        let selected: BTreeSet<_> = panels.iter().map(|p| p.group).collect();
        let retained: Vec<_> = self
            .triangles
            .iter()
            .enumerate()
            .filter(|(i, _)| !selected.contains(&self.polygroups[*i]))
            .map(|(_, t)| *t)
            .collect();
        if !retained.is_empty() {
            let first_new = retained.len();
            let n = self.vertices.len();
            let mut collision = Mesh {
                vertices: self.vertices.clone(),
                triangles: retained,
                ..Default::default()
            };
            collision.vertices.extend_from_slice(&display.vertices);
            collision
                .triangles
                .extend(display.triangles.iter().map(|t| t.map(|v| v + n)));
            super::fusion::validate_panel_faces(&collision,first_new,cancel)
                .map_err(|e| format!("Panel intersects nearby source geometry; reduce offset/thickness or increase gap. {e}"))?;
        }
        if symmetry.enabled {
            super::ModelingSymmetry::new(&display, symmetry)?;
        }
        check_cancel(cancel)?;
        Ok(PanelPreview {
            panels,
            outlines,
            display,
        })
    }
}

fn append_panel(out: &mut Mesh, mesh: &Mesh) {
    let n = out.vertices.len();
    out.vertices.extend_from_slice(&mesh.vertices);
    out.mask.extend_from_slice(&mesh.mask);
    out.materials.extend_from_slice(&mesh.materials);
    for (a, b) in out.vertex_groups.iter_mut().zip(&mesh.vertex_groups) {
        a.weights.extend_from_slice(&b.weights);
    }
    out.triangles
        .extend(mesh.triangles.iter().map(|t| t.map(|v| v + n)));
    out.quads
        .extend(mesh.quads.iter().map(|q| q.map(|v| v + n)));
    out.polygroups.extend_from_slice(&mesh.polygroups);
    out.hidden_faces.extend_from_slice(&mesh.hidden_faces);
    out.creases
        .extend(mesh.creases.iter().map(|e| e.map(|v| v + n)));
    let seams = out
        .shading_edges()
        .iter()
        .copied()
        .chain(mesh.shading_edges().iter().map(|e| e.map(|v| v + n)))
        .collect();
    out.set_shading_seams(seams);
    if !out.uv.corners.is_empty() {
        out.uv.corners.extend_from_slice(&mesh.uv.corners);
        out.uv.masks.extend_from_slice(&mesh.uv.masks);
        out.uv.removed.extend_from_slice(&mesh.uv.removed);
        out.uv
            .seams
            .extend(mesh.uv.seams.iter().map(|e| e.map(|v| v + n)));
        out.uv.topology = out.triangles.clone();
    }
}

impl PanelRegion {
    fn generate(&self, options: PanelOptions, cancel: &impl Fn() -> bool) -> Result<Mesh, String> {
        // The surface ends at the inset end of the bevel. The lateral wall
        // reaches gap/2, so changing its profile or resolution cannot change gap.
        let surface = self.rebuild_border(
            PanelBorder {
                gap: options.border.gap + 2. * options.bevel,
                ..options.border
            },
            cancel,
        )?;
        let boundaries = super::fusion::boundaries_cancellable(&surface, &cancel)?;
        let boundary: BTreeSet<_> = boundaries
            .iter()
            .flat_map(|b| b.vertices.iter().copied())
            .collect();
        let bevel_segments = if options.bevel > 0. {
            options.segments as usize
        } else {
            0
        };
        let rim_loops = 2 * bevel_segments + options.wall_loops as usize;
        let mut shell = surface.solidify_panel(options.thickness, rim_loops, cancel)?;
        let n = surface.vertices.len();
        let directions: Vec<_> = (0..n)
            .map(|v| (shell.vertices[v] - shell.vertices[v + n]).norm())
            .collect();
        for (v, &direction) in directions.iter().enumerate() {
            shell.vertices[v] = shell.vertices[v] + direction * options.offset;
            shell.vertices[v + n] = shell.vertices[v + n] + direction * options.offset;
        }
        let normals = surface.angle_weighted_normals();
        let mut inward = BTreeMap::new();
        for ring in boundaries {
            for (i, &v) in ring.vertices.iter().enumerate() {
                let p = surface.vertices[v];
                let a = surface.vertices
                    [ring.vertices[(i + ring.vertices.len() - 1) % ring.vertices.len()]];
                let b = surface.vertices[ring.vertices[(i + 1) % ring.vertices.len()]];
                let left = normals[v].cross((p - a).norm());
                let right = normals[v].cross((b - p).norm());
                let d = 1. + left.dot(right);
                if d < 0.1 {
                    return Err("Bevel cannot turn this sharp corner; reduce border smoothing or simplify the corner".into());
                }
                inward.insert(v, (left + right) * (1. / d));
            }
        }
        for (k, &v) in boundary.iter().enumerate() {
            check_cancel(cancel)?;
            let front = shell.vertices[v];
            let back = shell.vertices[v + n];
            let side_front = front - inward[&v] * options.bevel;
            let side_back = back - inward[&v] * options.bevel;
            let top = side_front - directions[v] * options.bevel;
            let bottom = side_back + directions[v] * options.bevel;
            let curve = |a: Vec3, corner: Vec3, b: Vec3, t: f32| match options.profile {
                PanelProfile::Flat => a * (1. - t) + b * t,
                PanelProfile::Rounded => {
                    super::modeling::bevel::rounded_bevel_profile(a, corner, b, t)
                }
            };
            for layer in 1..=rim_loops {
                let p = if bevel_segments > 0 && layer <= bevel_segments {
                    curve(front, side_front, top, layer as f32 / bevel_segments as f32)
                } else if bevel_segments > 0 && layer > bevel_segments + options.wall_loops as usize
                {
                    let t = (layer - bevel_segments - options.wall_loops as usize - 1) as f32
                        / bevel_segments as f32;
                    curve(bottom, side_back, back, t)
                } else {
                    let t = (layer - bevel_segments) as f32 / (options.wall_loops + 1) as f32;
                    top * (1. - t) + bottom * t
                };
                shell.vertices[2 * n + k * rim_loops + layer - 1] = p;
            }
        }
        for &[a, b, c] in &surface.triangles {
            let normal = (surface.vertices[b] - surface.vertices[a])
                .cross(surface.vertices[c] - surface.vertices[a]);
            for base in [0, n] {
                if (shell.vertices[b + base] - shell.vertices[a + base])
                    .cross(shell.vertices[c + base] - shell.vertices[a + base])
                    .dot(normal)
                    <= 0.
                {
                    return Err(
                        "Normal offset folds the panel surface; reduce offset or thickness".into(),
                    );
                }
            }
        }
        // New faces inherit their source panel ID; interpolated vertex attributes
        // come from the outline, while new UV faces remain explicitly unmapped.
        shell.polygroups.fill(self.group);
        super::modeling::manifold(&shell)?;
        shell.validate_closed()?;
        for quad in &shell.quads {
            let [a, b, c, d] = quad.map(|v| shell.vertices[v]);
            if (b - a).cross(c - a).dot((c - a).cross(d - a)) <= 0. {
                return Err(
                    "Panel folds a quad; reduce thickness, offset, bevel or border width".into(),
                );
            }
        }
        super::fusion::validate_panel_faces(&shell, 0, cancel)
            .map_err(|e| format!("Reduce thickness, offset, bevel or border width. {e}"))?;
        Ok(shell)
    }
}

#[cfg(all(test, not(feature = "focused-retopology")))]
mod tests {
    use super::*;
    use crate::sculpt::{GroupPurpose, Primitive, PrimitiveKind, VertexGroup};

    fn grouped_grid() -> Mesh {
        let mut grid = Primitive::for_kind(PrimitiveKind::Grid);
        grid.divisions = [3, 3, 1];
        let mut mesh = grid.mesh().unwrap();
        mesh.vertex_groups.push(VertexGroup {
            purpose: GroupPurpose::Selection,
            visible: true,
            name: "Plate".into(),
            weights: vec![0.; mesh.vertices.len()],
        });
        mesh.polygroups = mesh
            .triangles
            .iter()
            .map(|t| {
                let center = t
                    .iter()
                    .map(|&v| mesh.vertices[v])
                    .fold(super::super::Vec3::default(), |s, p| s + p)
                    * (1. / 3.);
                usize::from(center.0.abs() > 0.2 || center.1.abs() > 0.2)
            })
            .collect();
        mesh.mask.fill(0.25);
        mesh.uv.topology = mesh.triangles.clone();
        mesh.uv.corners = mesh
            .triangles
            .iter()
            .map(|t| {
                t.map(|v| {
                    let p = mesh.vertices[v];
                    [(p.0 + 1.) * 0.5, (p.1 + 1.) * 0.5]
                })
            })
            .collect();
        mesh.uv.masks = vec![[0.2, 0.3, 0.4]; mesh.triangles.len()];
        mesh
    }

    #[test]
    fn panel_regions_keep_hole_loops_and_source_attributes_without_mutation() {
        let mesh = grouped_grid();
        let original = mesh.clone();
        let [panel] = mesh
            .panel_regions(&[1], false, &|| false)
            .unwrap()
            .try_into()
            .ok()
            .unwrap();
        assert_eq!(panel.group, 1);
        assert_eq!(panel.perimeters.len(), 2);
        let mut lengths: Vec<_> = panel.perimeters.iter().map(Vec::len).collect();
        lengths.sort_unstable();
        assert_eq!(lengths, vec![4, 12]);
        assert_eq!(panel.surface.quads.len(), 8);
        assert_eq!(panel.surface.vertex_groups[0].name, "Plate");
        assert!(panel.surface.mask.iter().all(|&m| m == 0.25));
        assert!(panel.surface.polygroups.iter().all(|&g| g == 1));
        assert!(panel.surface.uv.current(&panel.surface));
        panel
            .surface
            .solidify(0.05)
            .unwrap()
            .validate_closed()
            .unwrap();
        assert_eq!(mesh, original);
    }

    #[test]
    fn panel_regions_reject_hidden_protected_branching_and_cancelled_inputs() {
        let mut mesh = grouped_grid();
        assert!(mesh.panel_regions(&[0], false, &|| false).is_err());
        assert!(mesh.panel_regions(&[2], false, &|| false).is_err());
        assert!(mesh.panel_regions(&[1], true, &|| false).is_err());
        assert!(mesh.panel_regions(&[1], false, &|| true).is_err());
        mesh.hidden_faces = vec![false; mesh.triangles.len()];
        mesh.hidden_faces[0] = true;
        assert!(mesh.panel_regions(&[1], false, &|| false).is_err());
    }

    #[test]
    fn panel_regions_keep_disconnected_islands_as_separate_boundary_cycles() {
        let mut mesh = grouped_grid();
        mesh.polygroups = mesh
            .quads
            .iter()
            .flat_map(|q| {
                let x = q.iter().map(|&v| mesh.vertices[v].0).sum::<f32>() / 4.;
                [usize::from(x.abs() > 0.6); 2]
            })
            .collect();
        let regions = mesh.panel_regions(&[1], false, &|| false).unwrap();
        assert_eq!(regions[0].perimeters.len(), 2);
        assert_eq!(regions[0].surface.quads.len(), 6);
        regions[0]
            .surface
            .solidify(0.05)
            .unwrap()
            .validate_closed()
            .unwrap();
    }

    #[test]
    fn panel_regions_reject_islands_touching_at_one_vertex() {
        let mut grid = Primitive::for_kind(PrimitiveKind::Grid);
        grid.divisions = [2, 2, 1];
        let mut mesh = grid.mesh().unwrap();
        mesh.vertex_groups.push(VertexGroup {
            purpose: GroupPurpose::Selection,
            visible: true,
            name: "Touching".into(),
            weights: vec![0.; mesh.vertices.len()],
        });
        mesh.polygroups = mesh
            .triangles
            .iter()
            .map(|t| {
                let center = t
                    .iter()
                    .map(|&v| mesh.vertices[v])
                    .fold(super::super::Vec3::default(), |s, p| s + p)
                    * (1. / 3.);
                usize::from(center.0 * center.1 > 0.)
            })
            .collect();
        assert!(
            mesh.panel_regions(&[1], false, &|| false)
                .err()
                .unwrap()
                .contains("touching or branching")
        );
    }

    #[test]
    fn border_rebuild_welds_hole_and_outer_quad_strips_without_moving_interior() {
        let mesh = grouped_grid();
        let panel = mesh
            .panel_regions(&[1], false, &|| false)
            .unwrap()
            .remove(0);
        let border = panel
            .rebuild_border(
                PanelBorder {
                    gap: 0.04,
                    width: 0.03,
                    loops: 3,
                    smoothing: 0.,
                    ..PanelBorder::default()
                },
                &|| false,
            )
            .unwrap();
        let perimeter_vertices: BTreeSet<_> = panel.perimeters.iter().flatten().copied().collect();
        for (i, &position) in panel.surface.vertices.iter().enumerate() {
            if !perimeter_vertices.contains(&i) {
                assert_eq!(border.vertices[i], position);
            }
        }
        let edge_count = perimeter_vertices.len();
        assert_eq!(
            border.vertices.len(),
            panel.surface.vertices.len() + 3 * edge_count
        );
        assert_eq!(
            border.quads.len(),
            panel.surface.quads.len() + 3 * edge_count
        );
        assert!(border.uv.current(&border));
        assert_eq!(
            &border.uv.corners[..panel.surface.triangles.len()],
            &panel.surface.uv.corners
        );
        assert!(
            border.uv.removed[panel.surface.triangles.len()..]
                .iter()
                .all(|&r| r)
        );
        border.solidify(0.02).unwrap().validate_closed().unwrap();
    }

    #[test]
    fn border_rebuild_rejects_overwide_and_cancelled_without_touching_source() {
        let panel = grouped_grid()
            .panel_regions(&[1], false, &|| false)
            .unwrap()
            .remove(0);
        let before = panel.surface.clone();
        let options = PanelBorder {
            gap: 0.,
            width: 0.02,
            loops: 2,
            smoothing: 0.,
            ..PanelBorder::default()
        };
        assert!(panel.rebuild_border(options, &|| true).is_err());
        assert!(
            panel
                .rebuild_border(
                    PanelBorder {
                        width: 10.,
                        ..options
                    },
                    &|| false
                )
                .is_err()
        );
        assert_eq!(panel.surface, before);
    }
    fn bounds(mesh: &Mesh) -> (Vec3, Vec3) {
        mesh.vertices.iter().fold(
            (
                Vec3(f32::INFINITY, f32::INFINITY, f32::INFINITY),
                Vec3(f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY),
            ),
            |(a, b), p| {
                (
                    Vec3(a.0.min(p.0), a.1.min(p.1), a.2.min(p.2)),
                    Vec3(b.0.max(p.0), b.1.max(p.1), b.2.max(p.2)),
                )
            },
        )
    }
    fn clean_grid(divisions: u32, classify: impl Fn(Vec3) -> usize, curved: bool) -> Mesh {
        let mut p = Primitive::for_kind(PrimitiveKind::Grid);
        p.divisions = [divisions, divisions, 1];
        let mut mesh = p.mesh().unwrap();
        for name in ["Left plate", "Right plate"] {
            mesh.vertex_groups.push(VertexGroup {
                purpose: GroupPurpose::Selection,
                visible: true,
                name: name.into(),
                weights: vec![0.; mesh.vertices.len()],
            });
        }
        mesh.polygroups = mesh
            .quads
            .iter()
            .flat_map(|q| {
                let c = q.iter().fold(Vec3::default(), |a, &v| a + mesh.vertices[v]) * 0.25;
                [classify(c); 2]
            })
            .collect();
        if curved {
            for p in &mut mesh.vertices {
                p.2 = 0.12 * (p.0 * p.0 + p.1 * p.1);
            }
        }
        mesh
    }

    #[test]
    fn generated_panels_curved_adjacent_groups_have_closed_oriented_bevels_and_stable_dimensions() {
        let source = clean_grid(6, |p| if p.0 < 0. { 1 } else { 2 }, true);
        let before = source.clone();
        let mut reference = None;
        for loops in [1, 3, 6] {
            for profile in [PanelProfile::Flat, PanelProfile::Rounded] {
                let options = PanelOptions {
                    profile,
                    border: PanelBorder {
                        loops,
                        ..Default::default()
                    },
                    ..Default::default()
                };
                let result = source
                    .generate_panels(&[1, 2], options, true, &Default::default(), &|| false)
                    .unwrap();
                assert_eq!(result.panels.len(), 2);
                assert_eq!(result.outlines.len(), 2);
                for panel in &result.panels {
                    panel.mesh.validate_closed().unwrap();
                    super::super::modeling::manifold(&panel.mesh).unwrap();
                }
                let bounds = bounds(&result.display);
                if let Some((a, b)) = reference {
                    assert!(
                        (bounds.0 - a).len() < 0.003 && (bounds.1 - b).len() < 0.003,
                        "{bounds:?} vs {a:?}/{b:?}"
                    );
                } else {
                    reference = Some(bounds);
                }
            }
        }
        assert_eq!(source, before);
    }

    #[test]
    fn generated_panels_holes_islands_concavity_and_narrow_regions_or_actionable_rejection() {
        for source in [
            clean_grid(
                6,
                |p| usize::from(p.0.abs() > 0.3 || p.1.abs() > 0.3),
                false,
            ),
            clean_grid(6, |p| usize::from(p.0.abs() > 0.4), false),
            clean_grid(6, |p| usize::from(p.0 < 0.2 || p.1 < 0.2), false),
            clean_grid(8, |p| usize::from(p.0.abs() < 0.2), false),
        ] {
            let result = source
                .generate_panels(
                    &[1],
                    PanelOptions::default(),
                    true,
                    &Default::default(),
                    &|| false,
                )
                .unwrap();
            result.display.validate_closed().unwrap();
            let invalid = PanelOptions {
                border: PanelBorder {
                    width: 1.,
                    ..Default::default()
                },
                ..Default::default()
            };
            assert!(
                source
                    .generate_panels(&[1], invalid, true, &Default::default(), &|| false)
                    .is_err()
            );
        }
    }

    #[test]
    fn generated_panels_offset_thickness_profile_and_resolution_are_independent() {
        let source = clean_grid(3, |_| 1, false);
        let mut previous = None;
        for segments in [1, 2, 5, 8] {
            for profile in [PanelProfile::Flat, PanelProfile::Rounded] {
                let o = PanelOptions {
                    thickness: 0.12,
                    offset: 0.2,
                    bevel: 0.025,
                    segments,
                    profile,
                    ..Default::default()
                };
                let result = source
                    .generate_panels(&[1], o, true, &Default::default(), &|| false)
                    .unwrap();
                let b = bounds(&result.display);
                assert!((b.0.2 - 0.14).abs() < 1e-6 && (b.1.2 - 0.26).abs() < 1e-6);
                if let Some((low, high)) = previous {
                    assert!((b.0 - low).len() < 1e-5 && (b.1 - high).len() < 1e-5);
                }
                previous = Some(b);
            }
        }
    }

    #[test]
    fn generated_panels_transfer_attributes_and_retain_interior_connectivity() {
        let mut source = grouped_grid();
        source.materials = vec![super::super::Material::default(); source.vertices.len()];
        source.creases = vec![source.polygon_edges()[0]];
        source.uv.seams = source.creases.clone();
        let panel = source
            .panel_regions(&[1], false, &|| false)
            .unwrap()
            .remove(0);
        let generated = source
            .generate_panels(
                &[1],
                PanelOptions::default(),
                false,
                &Default::default(),
                &|| false,
            )
            .unwrap()
            .panels
            .remove(0)
            .mesh;
        assert!(generated.uv.current(&generated));
        assert_eq!(
            &generated.triangles[..panel.surface.triangles.len()],
            &panel.surface.triangles
        );
        assert_eq!(
            &generated.uv.corners[..panel.surface.triangles.len()],
            &panel.surface.uv.corners
        );
        assert_eq!(
            &generated.materials[..panel.surface.vertices.len()],
            &panel.surface.materials
        );
        assert!(generated.mask.iter().all(|&m| m == 0.25));
        assert!(generated.polygroups.iter().all(|&g| g == 1));
        assert!(generated.uv.removed.iter().any(|&r| r));
        for e in &panel.surface.creases {
            assert!(generated.creases.contains(e));
        }
    }

    #[test]
    fn generated_panels_smooth_jagged_outline_while_pinning_deliberate_corners() {
        let mut source = clean_grid(8, |_| 1, false);
        for p in &mut source.vertices {
            if p.1 > 0.99 && p.0.abs() < 0.99 {
                p.1 += if ((p.0 + 1.) * 4.).round() as i32 % 2 == 0 {
                    0.05
                } else {
                    -0.05
                };
            }
        }
        let region = source
            .panel_regions(&[1], true, &|| false)
            .unwrap()
            .remove(0);
        let original = region
            .rebuild_border(PanelBorder::default(), &|| false)
            .unwrap();
        let smoothed = region
            .rebuild_border(
                PanelBorder {
                    smoothing: 1.,
                    corner_angle: 75.,
                    ..Default::default()
                },
                &|| false,
            )
            .unwrap();
        assert_ne!(original.vertices, smoothed.vertices);
        // Interior vertices outside the reconstructed one-ring remain byte exact.
        let boundary: BTreeSet<_> = region.perimeters.iter().flatten().copied().collect();
        for (v, &p) in region.surface.vertices.iter().enumerate() {
            if !boundary.contains(&v) {
                assert_eq!(smoothed.vertices[v], p);
            }
        }
        smoothed.solidify(0.04).unwrap().validate_closed().unwrap();
    }

    #[test]
    fn generated_panels_symmetry_cancellation_and_capacity_are_transactional() {
        let mesh = clean_grid(6, |p| if p.0 < 0. { 1 } else { 2 }, false);
        let symmetry = super::super::Symmetry {
            enabled: true,
            ..Default::default()
        };
        let result = mesh
            .generate_panels(&[1], PanelOptions::default(), true, &symmetry, &|| false)
            .unwrap();
        assert_eq!(result.panels.len(), 2);
        let calls = std::cell::Cell::new(0);
        assert!(
            mesh.generate_panels(&[1], PanelOptions::default(), true, &symmetry, &|| {
                calls.set(calls.get() + 1);
                calls.get() > 30
            })
            .is_err()
        );
        super::super::capacity::with_test_capacity(
            super::super::capacity::MeshCapacity {
                vertices: 80,
                triangles: 200,
            },
            || {
                let e = mesh
                    .generate_panels(
                        &[1],
                        PanelOptions::default(),
                        true,
                        &Default::default(),
                        &|| false,
                    )
                    .err()
                    .unwrap();
                assert!(e.contains("budget") || e.contains("limit"), "{e}");
            },
        );
        let mut broken = mesh.clone();
        broken.polygroups[0] = 0;
        broken.polygroups[1] = 0;
        assert!(
            broken
                .generate_panels(&[1], PanelOptions::default(), true, &symmetry, &|| false)
                .is_err()
        );
    }

    #[test]
    fn panels_detect_crossings_protection_stale_uv_and_folded_offsets() {
        let mut source = clean_grid(4, |_| 1, false);
        let before = source.clone();
        source.mask[0] = 1.;
        assert!(
            source
                .generate_panels(
                    &[1],
                    PanelOptions::default(),
                    true,
                    &Default::default(),
                    &|| false
                )
                .is_err()
        );
        source = before.clone();
        source.hidden_faces = vec![false; source.triangles.len()];
        source.hidden_faces[0] = true;
        assert!(
            source
                .generate_panels(
                    &[1],
                    PanelOptions::default(),
                    false,
                    &Default::default(),
                    &|| false
                )
                .is_err()
        );
        source = before.clone();
        source.uv.topology = source.triangles.clone();
        source.uv.corners = vec![[[0.; 2]; 3]; source.triangles.len()];
        source.uv.topology.reverse();
        assert!(
            source
                .generate_panels(
                    &[1],
                    PanelOptions::default(),
                    false,
                    &Default::default(),
                    &|| false
                )
                .is_err()
        );
        let mut crossing = before.clone();
        let mut other = before.clone();
        for p in &mut other.vertices {
            *p = Vec3(p.2, p.1, p.0);
        }
        append_panel(&mut crossing, &other);
        assert!(
            crossing
                .generate_panels(
                    &[1],
                    PanelOptions::default(),
                    true,
                    &Default::default(),
                    &|| false
                )
                .is_err()
        );
        let curved = clean_grid(8, |_| 1, true);
        assert!(
            curved
                .generate_panels(
                    &[1],
                    PanelOptions {
                        offset: 8.,
                        ..Default::default()
                    },
                    true,
                    &Default::default(),
                    &|| false
                )
                .is_err()
        );
        assert_eq!(before, clean_grid(4, |_| 1, false));
    }
    #[test]
    fn panels_dense_preview_cancel_and_subdivision_keep_closed_topology() {
        let source = clean_grid(100, |_| 1, true);
        let before = source.clone();
        let calls = std::cell::Cell::new(0);
        let start = std::time::Instant::now();
        let error = source
            .generate_panels(
                &[1],
                PanelOptions {
                    border: PanelBorder {
                        width: 0.004,
                        gap: 0.002,
                        ..Default::default()
                    },
                    bevel: 0.001,
                    thickness: 0.01,
                    offset: 0.01,
                    ..Default::default()
                },
                true,
                &Default::default(),
                &|| {
                    calls.set(calls.get() + 1);
                    calls.get() > 150
                },
            )
            .err()
            .unwrap();
        assert!(error.contains("cancel"), "{error}");
        eprintln!("20,000-triangle panel cancellation: {:?}", start.elapsed());
        assert_eq!(source, before);
        let source = clean_grid(5, |_| 1, true);
        let generated = source
            .generate_panels(
                &[1],
                PanelOptions::default(),
                true,
                &Default::default(),
                &|| false,
            )
            .unwrap();
        let smooth = generated.display.smooth_subdivide(1., &|| false).unwrap();
        smooth.validate_closed().unwrap();
        super::super::modeling::manifold(&smooth).unwrap();
        super::super::fusion::validate_panel_faces(&smooth, 0, &|| false).unwrap();
    }

    #[test]
    fn panels_from_adjacent_groups_on_a_closed_sphere_follow_curvature() {
        let mut primitive = Primitive::for_kind(PrimitiveKind::UvSphere);
        primitive.segments = 24;
        primitive.rings = 12;
        let mut source = primitive.mesh().unwrap();
        for name in ["Armour left", "Armour right"] {
            source.vertex_groups.push(VertexGroup {
                purpose: GroupPurpose::Selection,
                visible: true,
                name: name.into(),
                weights: vec![0.; source.vertices.len()],
            });
        }
        let polygons = source.polygons();
        let triangles = super::super::uv::polygon_triangles(&source);
        source.polygroups = vec![0; source.triangles.len()];
        for (i, p) in polygons.iter().enumerate() {
            let center = p
                .vertices
                .iter()
                .fold(Vec3::default(), |s, &v| s + source.vertices[v])
                * (1. / p.vertices.len() as f32);
            let group = if center.2 > 0.3 && center.1.abs() < 0.5 {
                if center.0 < 0. { 1 } else { 2 }
            } else {
                0
            };
            for &f in &triangles[i] {
                source.polygroups[f] = group;
            }
        }
        source.validate_closed().unwrap();
        let result = source
            .generate_panels(
                &[1, 2],
                PanelOptions {
                    border: PanelBorder {
                        width: 0.025,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                true,
                &Default::default(),
                &|| false,
            )
            .unwrap();
        assert_eq!(result.panels.len(), 2);
        result.display.validate_closed().unwrap();
        assert!(
            result
                .display
                .vertices
                .iter()
                .all(|p| p.len() > 0.97 && p.len() < 1.13)
        );
    }
}
