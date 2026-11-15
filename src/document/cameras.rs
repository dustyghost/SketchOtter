use super::Document;
use crate::saved_camera::{self, SavedCamera, View};

impl Document {
    pub fn saved_cameras(&self) -> &[SavedCamera] {
        &self.saved_cameras
    }

    // Bookmarks are project metadata, like reference-image placements. They
    // survive geometry Undo/Redo without copying a mesh or invalidating caches.
    pub(crate) fn save_camera(&mut self, name: String, view: View) -> Result<(), String> {
        self.edit_cameras(|cameras| {
            cameras.push(SavedCamera {
                name: name.trim().into(),
                view,
            });
            Ok(())
        })
    }

    pub(crate) fn update_camera(&mut self, index: usize, view: View) -> Result<(), String> {
        self.edit_cameras(|cameras| {
            cameras
                .get_mut(index)
                .ok_or("Saved camera no longer exists")?
                .view = view;
            Ok(())
        })
    }

    pub(crate) fn rename_camera(&mut self, index: usize, name: String) -> Result<(), String> {
        self.edit_cameras(|cameras| {
            cameras
                .get_mut(index)
                .ok_or("Saved camera no longer exists")?
                .name = name.trim().into();
            Ok(())
        })
    }

    pub(crate) fn delete_camera(&mut self, index: usize) -> Result<(), String> {
        self.edit_cameras(|cameras| {
            if index >= cameras.len() {
                return Err("Saved camera no longer exists".into());
            }
            cameras.remove(index);
            Ok(())
        })
    }

    fn edit_cameras(
        &mut self,
        edit: impl FnOnce(&mut Vec<SavedCamera>) -> Result<(), String>,
    ) -> Result<(), String> {
        let mut cameras = self.saved_cameras.clone();
        edit(&mut cameras)?;
        saved_camera::validate(&cameras)?;
        if self.saved_cameras != cameras {
            self.saved_cameras = cameras;
            self.dirty = true;
        }
        Ok(())
    }
}
