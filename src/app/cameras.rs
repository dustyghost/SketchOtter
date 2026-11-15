use super::*;
use crate::command::CameraCommand;

impl App {
    pub(super) fn dispatch_camera(&mut self, command: CameraCommand) -> Result<(), String> {
        let viewport_size = || {
            self.ui
                .viewport_menu
                .viewport_rect
                .filter(|rect| rect.is_positive())
                .map(|rect| rect.size())
                .ok_or("Open the viewport before using saved cameras")
        };
        match command {
            CameraCommand::Save(name) => {
                self.document
                    .save_camera(name.clone(), self.camera.saved_view(viewport_size()?)?)?;
                self.ui.cameras.saved(name.trim().into());
                Ok(())
            }
            CameraCommand::Update(index) => {
                self.document
                    .update_camera(index, self.camera.saved_view(viewport_size()?)?)?;
                self.ui
                    .cameras
                    .updated(self.document.saved_cameras()[index].name.clone());
                Ok(())
            }
            CameraCommand::Rename(index, name) => {
                let old = self
                    .document
                    .saved_cameras()
                    .get(index)
                    .ok_or("Saved camera no longer exists")?
                    .name
                    .clone();
                self.document.rename_camera(index, name.clone())?;
                self.ui.cameras.renamed(&old, name.trim().into());
                Ok(())
            }
            CameraCommand::Delete(index) => {
                let name = self
                    .document
                    .saved_cameras()
                    .get(index)
                    .ok_or("Saved camera no longer exists")?
                    .name
                    .clone();
                self.document.delete_camera(index)?;
                self.ui.cameras.deleted(&name);
                Ok(())
            }
            CameraCommand::Recall(index) => {
                let view = self
                    .document
                    .saved_cameras()
                    .get(index)
                    .ok_or("Saved camera no longer exists")?
                    .view;
                let next = Camera::from_saved_view(view, viewport_size()?)?;
                self.change_camera(next);
                self.ui
                    .cameras
                    .recalled(self.document.saved_cameras()[index].name.clone());
                Ok(())
            }
            CameraCommand::Roll(radians) => {
                if !radians.is_finite() {
                    return Err("Camera roll must be a finite angle".into());
                }
                let mut next = self.camera;
                next.roll = radians % std::f32::consts::TAU;
                self.change_camera(next);
                Ok(())
            }
        }
    }

    fn change_camera(&mut self, next: Camera) {
        if self.camera == next {
            return;
        }
        // Preserve completed work and detach any screen-space gesture before
        // switching projection. Persistent tool drafts remain available.
        self.viewport.finish_transform_drag(&mut self.document);
        self.viewport.wait_for_release();
        self.camera = next;
    }
}
