//! Weld cleaned outline/support loops into the active surface, without a shell.
use super::*;

pub struct PanelCutPreview {
    pub mesh: Mesh,
    pub outlines: Vec<Vec<Vec3>>,
    pub groups: usize,
    pub added_vertices: usize,
}
impl Mesh {
    pub fn cut_panel_outlines(
        &self,
        groups: &[usize],
        mut border: PanelBorder,
        protect: bool,
        symmetry: &super::super::Symmetry,
        cancel: &impl Fn() -> bool,
    ) -> Result<PanelCutPreview, String> {
        // A cut has no gap or removed material: its outer edge stays welded to
        // neighbouring source faces. Border width describes its support strip.
        border.gap = 0.;
        border.validate()?;
        let groups = self.panel_group_ids(groups, symmetry, cancel)?;
        let regions = self.panel_regions(&groups, protect, cancel)?;
        let added: usize = regions
            .iter()
            .map(|r| r.perimeters.iter().map(Vec::len).sum::<usize>() * border.loops as usize)
            .sum();
        if self.vertices.len().saturating_add(added)
            > super::super::max_vertices().min(MAX_PANEL_OUTPUT)
            || self.triangles.len().saturating_add(added.saturating_mul(2))
                > super::super::max_triangles().min(MAX_PANEL_OUTPUT)
        {
            return Err("Panel cut exceeds the 500,000 vertex/triangle working budget; reduce support loops or use a lower source level".into());
        }
        let projection = super::super::projection::Surface::new(self, cancel)?;
        let removed: BTreeSet<_> = regions
            .iter()
            .flat_map(|r| r.source_faces.iter().copied())
            .collect();
        let face_key = |mut t: [usize; 3]| {
            t.sort_unstable();
            t
        };
        let removed_keys: BTreeSet<_> = removed
            .iter()
            .map(|&f| face_key(self.triangles[f]))
            .collect();
        let mut out = self.clone();
        out.triangles = self
            .triangles
            .iter()
            .enumerate()
            .filter_map(|(f, &t)| (!removed.contains(&f)).then_some(t))
            .collect();
        out.polygroups = self
            .polygroups
            .iter()
            .enumerate()
            .filter_map(|(f, &g)| (!removed.contains(&f)).then_some(g))
            .collect();
        out.hidden_faces = self
            .hidden_faces
            .iter()
            .enumerate()
            .filter_map(|(f, &v)| (!removed.contains(&f)).then_some(v))
            .collect();
        out.quads.retain(|&[a, b, c, d]| {
            ![[a, b, c], [a, c, d], [a, b, d], [b, c, d]]
                .iter()
                .any(|&t| removed_keys.contains(&face_key(t)))
        });
        let mapped = self.uv.current(self);
        if mapped {
            out.uv.corners = self
                .uv
                .corners
                .iter()
                .enumerate()
                .filter_map(|(f, &v)| (!removed.contains(&f)).then_some(v))
                .collect();
            out.uv.masks = self
                .uv
                .masks
                .iter()
                .enumerate()
                .filter_map(|(f, &v)| (!removed.contains(&f)).then_some(v))
                .collect();
            out.uv.removed = if self.uv.removed.is_empty() {
                vec![false; out.triangles.len()]
            } else {
                self.uv
                    .removed
                    .iter()
                    .enumerate()
                    .filter_map(|(f, &v)| (!removed.contains(&f)).then_some(v))
                    .collect()
            };
        }
        let mut seams = self.shading_edges().to_vec();
        let mut placed = BTreeMap::<usize, Vec3>::new();
        let mut outlines = Vec::new();
        for region in &regions {
            check_cancel(cancel)?;
            let rebuilt = region
                .rebuild_border_on_surface(border, &projection, cancel)
                .map_err(|e| format!("Group {}: {e}", region.group))?;
            let mut remap = vec![usize::MAX; rebuilt.vertices.len()];
            remap[..region.surface.vertices.len()].copy_from_slice(&region.source_vertices);
            let mut cursor = region.surface.vertices.len();
            for cycle in &region.perimeters {
                let mut outline = Vec::new();
                for &v in cycle {
                    check_cancel(cancel)?;
                    let original = region.source_vertices[v];
                    let position = rebuilt.vertices[cursor];
                    if let Some(&previous) = placed.get(&original) {
                        if (previous - position).len() > 1e-6 {
                            return Err("Adjacent cut outlines disagree at a shared vertex; reduce smoothing or cut the groups separately".into());
                        }
                    } else {
                        placed.insert(original, position);
                        out.vertices[original] = position;
                    }
                    outline.push(region.surface.vertices[v]);
                    // The existing boundary belongs to the outside faces. Move
                    // the cap's boundary to a new vertex and weld the new outer
                    // ring to that existing boundary, preserving one manifold.
                    remap[v] = out.append_blend(&rebuilt, &[(v, 1.)], rebuilt.vertices[v]);
                    remap[cursor] = original;
                    for layer in 1..border.loops as usize {
                        remap[cursor + layer] = out.append_blend(
                            &rebuilt,
                            &[(cursor + layer, 1.)],
                            rebuilt.vertices[cursor + layer],
                        );
                    }
                    cursor += border.loops as usize;
                }
                outlines.push(outline);
            }
            out.triangles
                .extend(rebuilt.triangles.iter().map(|t| t.map(|v| remap[v])));
            out.quads
                .extend(rebuilt.quads.iter().map(|q| q.map(|v| remap[v])));
            out.polygroups.extend_from_slice(&rebuilt.polygroups);
            if !out.hidden_faces.is_empty() {
                out.hidden_faces
                    .extend(std::iter::repeat_n(false, rebuilt.triangles.len()));
            }
            if mapped {
                out.uv.corners.extend_from_slice(&rebuilt.uv.corners);
                if !self.uv.masks.is_empty() {
                    out.uv.masks.extend_from_slice(&rebuilt.uv.masks);
                }
                out.uv.removed.extend_from_slice(&rebuilt.uv.removed);
            }
            out.creases
                .extend(rebuilt.creases.iter().map(|e| e.map(|v| remap[v])));
            out.uv
                .seams
                .extend(rebuilt.uv.seams.iter().map(|e| e.map(|v| remap[v])));
            seams.extend(rebuilt.shading_edges().iter().map(|e| e.map(|v| remap[v])));
        }
        repair_adjoining_faces(self, &mut out, &removed, cancel)?;
        let edges: BTreeSet<_> = out
            .triangles
            .iter()
            .flat_map(|&[a, b, c]| [edge(a, b), edge(b, c), edge(c, a)])
            .collect();
        out.uv.seams.retain(|e| edges.contains(e));
        out.uv.seams.sort_unstable();
        out.uv.seams.dedup();
        seams.retain(|e| edges.contains(e));
        out.set_shading_seams(seams);
        out.retain_creases();
        if mapped {
            out.uv.topology = out.triangles.clone();
        }
        // Preserve explicit unmapped state instead of triggering document UV fallback.
        if !self.uv.is_empty() && self.uv.corners.is_empty() {
            out.uv.topology = out.triangles.clone();
            out.uv.corners = vec![[[0.; 2]; 3]; out.triangles.len()];
            out.uv.removed = vec![true; out.triangles.len()];
        }
        out.validate()?;
        super::super::modeling::manifold(&out)?;
        if self.validate_closed().is_ok() {
            out.validate_closed()?;
        }
        super::super::fusion::validate_panel_faces(&out,0,cancel)
            .map_err(|e| format!("The cut crosses or folds the source surface; reduce smoothing or support width. {e}"))?;
        self.check_model_protection(&out, protect)?;
        if symmetry.enabled {
            super::super::ModelingSymmetry::new(&out, symmetry)?;
        }
        check_cancel(cancel)?;
        Ok(PanelCutPreview {
            added_vertices: out.vertices.len() - self.vertices.len(),
            mesh: out,
            outlines,
            groups: regions.len(),
        })
    }
}

// Rounding an outline can turn an outside ear through its old diagonal.
// Flip only adjoining retained faces, never an outline/support edge, seam or
// category boundary. This changes local connectivity without moving the next
// row of source vertices or remeshing the rest of the object.
fn repair_adjoining_faces(
    source: &Mesh,
    mesh: &mut Mesh,
    removed: &BTreeSet<usize>,
    cancel: &impl Fn() -> bool,
) -> Result<(), String> {
    let originals: Vec<_> = source
        .triangles
        .iter()
        .enumerate()
        .filter(|(f, _)| !removed.contains(f))
        .map(|(_, &t)| t)
        .collect();
    let reference: Vec<_> = originals
        .iter()
        .map(|&[a, b, c]| {
            (source.vertices[b] - source.vertices[a])
                .cross(source.vertices[c] - source.vertices[a])
                .norm()
        })
        .collect();
    let mut reference = reference;
    let normal = |m: &Mesh, [a, b, c]: [usize; 3]| {
        (m.vertices[b] - m.vertices[a]).cross(m.vertices[c] - m.vertices[a])
    };
    let mut pending: std::collections::VecDeque<_> = (0..originals.len())
        .filter(|&f| normal(mesh, mesh.triangles[f]).dot(reference[f]) <= 0.)
        .collect();
    if pending.is_empty() {
        return Ok(());
    }
    let blocked: BTreeSet<_> = mesh
        .creases
        .iter()
        .chain(&mesh.uv.seams)
        .chain(source.shading_edges())
        .copied()
        .collect();
    let mut incidence = BTreeMap::<[usize; 2], Vec<usize>>::new();
    for (f, &[a, b, c]) in mesh.triangles.iter().enumerate() {
        if f % 256 == 0 {
            check_cancel(cancel)?;
        }
        for e in [edge(a, b), edge(b, c), edge(c, a)] {
            incidence.entry(e).or_default().push(f);
        }
    }
    let mapped = source.uv.current(source);
    let masks = mapped && !source.uv.masks.is_empty();
    let mut changed = BTreeSet::new();
    let mut attempts = pending.len().saturating_mul(4);
    while let Some(f) = pending.pop_front() {
        check_cancel(cancel)?;
        if normal(mesh, mesh.triangles[f]).dot(reference[f]) > 0. {
            continue;
        }
        if attempts == 0 {
            return Err("The cleaned cut folds an adjoining face that cannot be retessellated safely; reduce smoothing or support width".into());
        }
        attempts -= 1;
        let a = mesh.triangles[f];
        let mut flipped = false;
        for k in 0..3 {
            let (u, v, x) = (a[k], a[(k + 1) % 3], a[(k + 2) % 3]);
            let e = edge(u, v);
            if blocked.contains(&e) {
                continue;
            }
            let Some(faces) = incidence.get(&e) else {
                continue;
            };
            if faces.len() != 2 {
                continue;
            }
            let other = if faces[0] == f { faces[1] } else { faces[0] };
            if other >= originals.len()
                || !mesh.face_visible(f)
                || !mesh.face_visible(other)
                || mesh.polygroups[f] != mesh.polygroups[other]
            {
                continue;
            }
            let b = mesh.triangles[other];
            let y = *b
                .iter()
                .find(|&&i| i != u && i != v)
                .ok_or("Invalid adjacent cut face")?;
            if x == y || incidence.contains_key(&edge(x, y)) {
                continue;
            }
            let n = (reference[f] + reference[other]).norm();
            let (first, second) = ([x, y, v], [y, x, u]);
            if normal(mesh, first).dot(n) <= 0. || normal(mesh, second).dot(n) <= 0. {
                continue;
            }
            let ai = |i| a.iter().position(|&v| v == i).unwrap();
            let bi = |i| b.iter().position(|&v| v == i).unwrap();
            if mapped {
                let (ca, cb) = (mesh.uv.corners[f], mesh.uv.corners[other]);
                if mesh.uv.removed[f] != mesh.uv.removed[other]
                    || [u, v].iter().any(|&i| ca[ai(i)] != cb[bi(i)])
                {
                    continue;
                }
                if masks
                    && [u, v]
                        .iter()
                        .any(|&i| mesh.uv.masks[f][ai(i)] != mesh.uv.masks[other][bi(i)])
                {
                    continue;
                }
                let uv = |t: [usize; 3]| {
                    t.map(|i| {
                        a.iter()
                            .position(|&v| v == i)
                            .map_or_else(|| cb[bi(i)], |j| ca[j])
                    })
                };
                mesh.uv.corners[f] = uv(first);
                mesh.uv.corners[other] = uv(second);
                if masks {
                    let (ma, mb) = (mesh.uv.masks[f], mesh.uv.masks[other]);
                    let mask = |t: [usize; 3]| {
                        t.map(|i| {
                            a.iter()
                                .position(|&v| v == i)
                                .map_or_else(|| mb[bi(i)], |j| ma[j])
                        })
                    };
                    mesh.uv.masks[f] = mask(first);
                    mesh.uv.masks[other] = mask(second);
                }
            }
            for (id, [a, b, c]) in [(f, a), (other, b)] {
                let mut key = [a, b, c];
                key.sort_unstable();
                changed.insert(key);
                for e in [edge(a, b), edge(b, c), edge(c, a)] {
                    let faces = incidence.get_mut(&e).unwrap();
                    faces.retain(|&v| v != id);
                    if faces.is_empty() {
                        incidence.remove(&e);
                    }
                }
            }
            mesh.triangles[f] = first;
            mesh.triangles[other] = second;
            for id in [f, other] {
                let [a, b, c] = mesh.triangles[id];
                for e in [edge(a, b), edge(b, c), edge(c, a)] {
                    incidence.entry(e).or_default().push(id);
                }
            }
            reference[f] = n;
            reference[other] = n;
            flipped = true;
            break;
        }
        if !flipped {
            pending.push_back(f);
        }
    }
    mesh.quads.retain(|&[a, b, c, d]| {
        ![[a, b, c], [a, c, d], [a, b, d], [b, c, d]]
            .into_iter()
            .any(|mut t| {
                t.sort_unstable();
                changed.contains(&t)
            })
    });
    Ok(())
}

#[cfg(all(test, not(feature = "focused-retopology")))]
mod tests {
    use super::*;
    use crate::sculpt::{GroupPurpose, Primitive, PrimitiveKind, VertexGroup};
    pub(super) fn grid(group: impl Fn(Vec3) -> usize) -> Mesh {
        let mut p = Primitive::for_kind(PrimitiveKind::Grid);
        p.divisions = [8, 8, 1];
        let mut m = p.mesh().unwrap();
        for name in ["First", "Second"] {
            m.vertex_groups.push(VertexGroup {
                name: name.into(),
                purpose: GroupPurpose::Selection,
                visible: true,
                weights: vec![0.; m.vertices.len()],
            });
        }
        m.polygroups = m
            .quads
            .iter()
            .flat_map(|q| {
                let center = q.iter().fold(Vec3::default(), |s, &v| s + m.vertices[v]) * 0.25;
                [group(center); 2]
            })
            .collect();
        m
    }
    fn options() -> PanelBorder {
        PanelBorder {
            width: 0.035,
            ..Default::default()
        }
    }
    #[test]
    fn panels_cut_welds_loops_into_source_and_preserves_planar_shape_uvs_and_attributes() {
        let mut source = grid(|p| usize::from(p.0.abs() < 0.6 && p.1.abs() < 0.6));
        source.mask.fill(0.25);
        source.uv.topology = source.triangles.clone();
        source.uv.corners = source
            .triangles
            .iter()
            .map(|t| t.map(|v| [source.vertices[v].0, source.vertices[v].1]))
            .collect();
        source.uv.masks = vec![[0.1, 0.2, 0.3]; source.triangles.len()];
        let before = source.clone();
        let region = source
            .panel_regions(&[1], false, &|| false)
            .unwrap()
            .remove(0);
        let boundary: usize = region.perimeters.iter().map(Vec::len).sum();
        for loops in [1, 2, 4] {
            let cut = source
                .cut_panel_outlines(
                    &[1],
                    PanelBorder { loops, ..options() },
                    false,
                    &Default::default(),
                    &|| false,
                )
                .unwrap();
            assert_eq!(cut.added_vertices, boundary * loops as usize);
            assert_eq!(
                cut.mesh.triangles.len(),
                source.triangles.len() + 2 * boundary * loops as usize
            );
            assert_eq!(
                &cut.mesh.vertices[..source.vertices.len()],
                &source.vertices
            );
            assert!(cut.mesh.vertices.iter().all(|v| v.2 == 0.));
            assert!(cut.mesh.mask.iter().all(|&m| m == 0.25));
            assert!(cut.mesh.uv.current(&cut.mesh));
            for (&t, &uv) in source.triangles.iter().zip(&source.uv.corners) {
                if let Some(i) = cut.mesh.triangles.iter().position(|&v| v == t) {
                    assert_eq!(cut.mesh.uv.corners[i], uv);
                }
            }
            assert_eq!(
                super::super::super::fusion::boundaries(&cut.mesh)
                    .unwrap()
                    .len(),
                1
            );
            let ignored_gap = source
                .cut_panel_outlines(
                    &[1],
                    PanelBorder {
                        gap: 20.,
                        loops,
                        ..options()
                    },
                    false,
                    &Default::default(),
                    &|| false,
                )
                .unwrap();
            assert_eq!(cut.mesh, ignored_gap.mesh);
        }
        assert_eq!(source, before);
    }
    #[test]
    fn panels_cut_handles_holes_islands_and_adjacent_groups_without_opening_surface() {
        for source in [
            grid(|p| {
                usize::from(
                    p.0.abs() < 0.8 && p.1.abs() < 0.8 && (p.0.abs() > 0.25 || p.1.abs() > 0.25),
                )
            }),
            grid(|p| usize::from(p.0.abs() > 0.25 && p.1.abs() < 0.5)),
            grid(|p| {
                if p.1.abs() < 0.6 {
                    if p.0 < 0. { 1 } else { 2 }
                } else {
                    0
                }
            }),
        ] {
            let groups: Vec<_> = source
                .polygroups
                .iter()
                .copied()
                .filter(|&g| g > 0)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();
            let cut = source
                .cut_panel_outlines(&groups, options(), true, &Default::default(), &|| false)
                .unwrap();
            assert_eq!(
                super::super::super::fusion::boundaries(&cut.mesh)
                    .unwrap()
                    .len(),
                1
            );
            assert_eq!(cut.groups, groups.len());
            cut.mesh.validate().unwrap();
        }
    }
    #[test]
    fn panels_cut_rejects_protected_hidden_overwide_cancelled_and_over_budget_inputs() {
        let source = grid(|p| usize::from(p.0.abs() < 0.6 && p.1.abs() < 0.6));
        let before = source.clone();
        assert!(
            source
                .cut_panel_outlines(&[1], options(), true, &Default::default(), &|| true)
                .err()
                .unwrap()
                .contains("cancel")
        );
        assert!(
            source
                .cut_panel_outlines(
                    &[1],
                    PanelBorder {
                        width: 2.,
                        ..options()
                    },
                    true,
                    &Default::default(),
                    &|| false
                )
                .is_err()
        );
        let mut protected = source.clone();
        protected.mask.fill(0.5);
        assert!(
            protected
                .cut_panel_outlines(&[1], options(), true, &Default::default(), &|| false)
                .is_err()
        );
        assert!(
            protected
                .cut_panel_outlines(&[1], options(), false, &Default::default(), &|| false)
                .is_ok()
        );
        let mut hidden = source.clone();
        hidden.hidden_faces = hidden.polygroups.iter().map(|&g| g == 1).collect();
        assert!(
            hidden
                .cut_panel_outlines(&[1], options(), false, &Default::default(), &|| false)
                .is_err()
        );
        let mut huge = source.clone();
        huge.triangles
            .resize(MAX_PANEL_INPUT + 1, source.triangles[0]);
        huge.polygroups.resize(MAX_PANEL_INPUT + 1, 1);
        huge.quads.clear();
        assert!(
            huge.cut_panel_outlines(&[1], options(), false, &Default::default(), &|| false)
                .is_err()
        );
        assert_eq!(source, before);
    }
}

#[cfg(all(test, not(feature = "focused-retopology")))]
mod curved_tests {
    use super::*;
    use crate::sculpt::{GroupPurpose, VertexGroup};
    #[test]
    fn panels_cut_polishes_painted_sphere_and_preserves_other_vertices_and_closure() {
        let patch: Mesh =
            serde_json::from_str(include_str!("../fixtures/panel-painted-boundary.json")).unwrap();
        let key = |mesh: &Mesh, t: [usize; 3]| {
            let mut p = t.map(|v| {
                let p = mesh.vertices[v];
                [
                    (p.0 * 1e6).round() as i32,
                    (p.1 * 1e6).round() as i32,
                    (p.2 * 1e6).round() as i32,
                ]
            });
            p.sort_unstable();
            p
        };
        let faces: BTreeSet<_> = patch.triangles.iter().map(|&t| key(&patch, t)).collect();
        let mut source = Mesh::sphere();
        source.vertex_groups.push(VertexGroup {
            name: "Painted patch".into(),
            purpose: GroupPurpose::Selection,
            visible: true,
            weights: vec![0.; source.vertices.len()],
        });
        source.polygroups = source
            .triangles
            .iter()
            .map(|&t| usize::from(faces.contains(&key(&source, t))))
            .collect();
        let before = source.clone();
        let region = source
            .panel_regions(&[1], true, &|| false)
            .unwrap()
            .remove(0);
        let boundary: BTreeSet<_> = region
            .perimeters
            .iter()
            .flatten()
            .map(|&v| region.source_vertices[v])
            .collect();
        for smoothing in [0., 0.7, 1.] {
            let options = source.fit_panel_border(Default::default());
            let result = source
                .cut_panel_outlines(
                    &[1],
                    PanelBorder {
                        smoothing,
                        corner_angle: 120.,
                        loops: 3,
                        ..options.border
                    },
                    true,
                    &Default::default(),
                    &|| false,
                )
                .unwrap();
            result.mesh.validate_closed().unwrap();
            for (v, &p) in source.vertices.iter().enumerate() {
                if !boundary.contains(&v) {
                    assert_eq!(result.mesh.vertices[v], p);
                }
            }
            assert!(
                result
                    .mesh
                    .vertices
                    .iter()
                    .all(|p| (0.995..1.001).contains(&p.len()))
            );
        }
        let mut mapped = source.clone();
        mapped.uv.topology = mapped.triangles.clone();
        mapped.uv.corners = mapped
            .triangles
            .iter()
            .map(|t| t.map(|v| [mapped.vertices[v].0, mapped.vertices[v].1]))
            .collect();
        mapped.uv.masks = vec![[0.25; 3]; mapped.triangles.len()];
        let options = mapped.fit_panel_border(Default::default());
        let border = PanelBorder {
            smoothing: 1.,
            corner_angle: 120.,
            ..options.border
        };
        let cut = mapped
            .cut_panel_outlines(&[1], border, true, &Default::default(), &|| false)
            .unwrap();
        assert!(cut.mesh.uv.current(&cut.mesh));
        let mut blocked = mapped.clone();
        blocked.uv.seams = blocked
            .triangles
            .iter()
            .flat_map(|&[a, b, c]| [edge(a, b), edge(b, c), edge(c, a)])
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let error = blocked
            .cut_panel_outlines(&[1], border, true, &Default::default(), &|| false)
            .err()
            .unwrap();
        assert!(error.contains("retessellated"), "{error}");
        assert_eq!(source, before);
    }
}

#[cfg(all(test, not(feature = "focused-retopology")))]
mod symmetry_tests {
    use super::*;
    #[test]
    fn panels_cut_includes_matching_groups_under_symmetry() {
        let source = super::tests::grid(|p| if p.0 < 0. { 1 } else { 2 });
        let symmetry = crate::sculpt::Symmetry {
            enabled: true,
            ..Default::default()
        };
        let cut = source
            .cut_panel_outlines(
                &[1],
                PanelBorder {
                    width: 0.035,
                    ..Default::default()
                },
                true,
                &symmetry,
                &|| false,
            )
            .unwrap();
        assert_eq!(cut.groups, 2);
        crate::sculpt::ModelingSymmetry::new(&cut.mesh, &symmetry).unwrap();
    }
}
