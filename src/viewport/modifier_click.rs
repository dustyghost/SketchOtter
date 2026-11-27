//! Shared mouse/pen tap arbitration. A tap never leaks a press into an editor.
use super::*;

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Tap {
    Model,
    Mask,
    Visibility,
    VertexGroup(VertexGroupTool),
    ClearMask,
    ClearVisibility,
    ClearSelection,
}
pub(super) struct Pending {
    start: Pos2,
    modifiers: egui::Modifiers,
    action: Tap,
    revision: u64,
    events: Vec<egui::Event>,
    group: Option<usize>,
}

impl Viewport {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn modifier_click(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        scene: &Scene,
        document: &mut Document,
        options: ViewportOptions,
        gizmo: egui::Rect,
    ) -> bool {
        let events = ui.input(|i| i.events.clone());
        let press = events.iter().find_map(|e| match e {
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers,
            } => Some((*pos, *modifiers)),
            _ => None,
        });
        if self.session.modifier_click.is_none()
            && self.session.gesture.is_none()
            && !self.modeling.dragging()
            && options.enabled
            && !options.edge_select
            && !ui.ctx().wants_keyboard_input()
            && !egui::Popup::is_any_open(ui.ctx())
            && let Some((pos, modifiers)) = press
            && !modifiers.mac_cmd
            && ((modifiers.ctrl && !modifiers.alt)
                || matches!(options.select, Some(SelectionTool::VertexGroup(_))))
            && response.rect.contains(pos)
            && !navigation::contains(response.rect, pos)
            && !gizmo.contains(pos)
            && ui.ctx().layer_id_at(pos) == Some(response.layer_id)
            // Handles win over modifier taps; placement tools also retain their clicks.
            && !(options.transform_mode != TransformMode::Sculpt && self.session.transform.can_drag(
                document, scene, options.transform_mode, options.transform_operation, pos))
            && !(options.transform_mode == TransformMode::Sculpt && self.session.primitive.can_drag(document, scene, pos))
            && !self.modeling.modifier_handle(document, scene, options.modeling, pos, ui.painter())
        {
            let action = if modifiers.ctrl
                && modifiers.shift
                && !modifiers.alt
                && scene.pick_model(pos).is_none()
            {
                options.clear_context
            } else if let Some(SelectionTool::VertexGroup(tool)) = options.select
                && (!modifiers.ctrl || modifiers.shift || modifiers.alt)
            {
                Some(Tap::VertexGroup(tool.reversed(modifiers.alt)))
            } else if modifiers.shift {
                options.invert_context
            } else {
                Some(Tap::Model)
            };
            if let Some(action) = action {
                self.session.modifier_click = Some(Pending {
                    start: pos,
                    modifiers,
                    action,
                    revision: document.display_revision(),
                    events: Vec::new(),
                    group: if matches!(action, Tap::VertexGroup(_)) {
                        scene.pick_polygroup(pos, document)
                    } else {
                        None
                    },
                });
            }
        }
        let Some(pending) = self.session.modifier_click.as_mut() else {
            return false;
        };
        // Pen/touch backends may remove the hover pointer immediately after a
        // completed tip release. Only losing it before release cancels the tap.
        let lost_before_release = events
            .iter()
            .take_while(|e| {
                !matches!(
                    e,
                    egui::Event::PointerButton {
                        button: egui::PointerButton::Primary,
                        pressed: false,
                        ..
                    }
                )
            })
            .any(|e| matches!(e, egui::Event::PointerGone));
        if !options.enabled
            || ui.ctx().wants_keyboard_input()
            || egui::Popup::is_any_open(ui.ctx())
            || document.display_revision() != pending.revision
            || ui.input(|i| !i.focused || i.key_pressed(egui::Key::Escape) || lost_before_release)
        {
            self.session.modifier_click = None;
            self.wait_for_release();
            return true;
        }
        let mut collecting = !pending.events.is_empty();
        let mut dragged = false;
        let mut release = None;
        for event in events {
            if matches!(
                event,
                egui::Event::PointerButton {
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    ..
                }
            ) {
                collecting = true;
            }
            if !collecting {
                continue;
            }
            let point = match event {
                egui::Event::PointerMoved(p) => Some(p),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers,
                } => {
                    release = Some((pos, modifiers));
                    Some(pos)
                }
                _ => None,
            };
            if let Some(p) = point {
                dragged |= p.distance(pending.start)
                    > ui.ctx().options(|o| o.input_options.max_click_dist);
            }
            pending.events.push(event);
            if release.is_some() {
                break;
            }
        }
        if dragged {
            // Replay the original press and all samples into the established drag
            // paths. Holds remain remappable; no tap action runs after this point.
            let pending = self.session.modifier_click.take().unwrap();
            self.session.resumed_drag = Some(pending.start);
            ui.input_mut(|i| i.events = pending.events);
            return false;
        }
        if let Some((pos, released)) = release {
            let pending = self.session.modifier_click.take().unwrap();
            let valid = response.rect.contains(pos)
                && response.clicked_by(egui::PointerButton::Primary)
                && ui.ctx().layer_id_at(pos) == Some(response.layer_id)
                && released.ctrl == pending.modifiers.ctrl
                && released.shift == pending.modifiers.shift
                && (!matches!(
                    pending.action,
                    Tap::ClearMask | Tap::ClearVisibility | Tap::ClearSelection
                ) || scene.pick_model(pos).is_none())
                && if matches!(pending.action, Tap::VertexGroup(_)) {
                    released.alt == pending.modifiers.alt
                } else {
                    !released.alt
                }
                && !released.mac_cmd;
            if valid {
                let result = match pending.action {
                    Tap::Mask => document.invert_mask(),
                    Tap::Visibility => document.invert_visibility(),
                    Tap::ClearMask => {
                        document.clear_mask();
                        self.modeling.accept_attribute_revision(document);
                        self.status = Some("Mask cleared".into());
                        Ok(())
                    }
                    Tap::ClearVisibility => document.show_all_faces().map(|()| {
                        self.modeling.accept_attribute_revision(document);
                        self.status = Some("All faces shown".into());
                    }),
                    Tap::ClearSelection => {
                        self.modeling.sync(document);
                        self.modeling.record_selection(document, Default::default());
                        self.status = Some("Component selection cleared".into());
                        Ok(())
                    }
                    Tap::VertexGroup(tool) => {
                        self.click_vertex_group(document, tool, pending.group)
                    }
                    Tap::Model => {
                        if let Some((owner, _, _)) = scene.pick_model(pos)
                            && let Some(index) = document.display_owner_model(owner)
                            && index != document.active_model_index()
                        {
                            self.session.transform = Default::default();
                            self.session.transform.space = options.transform_space;
                            if let Err(error) = document.select_submodel(index) {
                                self.status = Some(error);
                            }
                            self.array_selected = document.attached_array(index).is_some();
                        }
                        Ok(())
                    }
                };
                if let Err(error) = result {
                    self.status = Some(error);
                }
            }
        }
        true
    }
}
