# Masks, vertex groups, and selection

[Documentation home](../../README.md#documentation)

**Mask** a region to protect it from brushes. **Hide** it to work without seeing or picking it. **Group** it to recall a selection later.

- [Protect a region with a lasso](#example-protect-a-region-with-a-lasso)
- [Protect one side through the model](#protect-one-side-through-the-model)
- [Edit a mask](#mask-operations)
- [Split masked faces](#split-masked-geometry)
- [Save a region as a group](#vertex-groups)
- [Hide or isolate faces](#selection-and-visibility)
- [Split off hidden faces](#split-hidden-faces)

## Slice with the shared hold

1. Press **Z** (the default quick-menu key) and choose **Slice** beside Mask and Visibility.
2. Choose **Line** or **Path**, then **Vertex groups**, **Trim** or **Separate**. Trim and Separate offer **Cap cuts**; Trim also offers **Flip kept side**. These choices configure the next cut.
3. Hold **Ctrl+Shift** over an editable model. Draw and release a line to cut, or place a path and press **Enter** to apply it while holding Ctrl+Shift.
4. Release Ctrl+Shift to return to your previous sculpting or Modeling tool. Releasing it before applying discards an unfinished cut; **Escape** also cancels it. A completed cut is one Undo step.

Slice cuts through all depths and uses the existing [Slice rules](modeling.md#slice), including its mask, symmetry and subdivision limits. Right-drag orbit, middle-drag pan and wheel zoom remain available. The menu key and shared hold follow your configured bindings. Ctrl+Shift+Z retains Redo: open the chooser with Z before holding Ctrl+Shift. Ctrl+V retains its existing behaviour.

## Hold to mask

1. New preferences default to **Mask → Surface brush**: hold **Ctrl+Shift** and drag to paint immediately, without opening Z first. Press **Z** to choose **Mask**, **Visibility** or **Slice**. While Modeling is enabled, **Select** also offers [component region selection](modeling.md#select-many-components-with-a-region). That tab determines what **Ctrl+Shift** does. The selected mode is remembered after restarting; existing saved choices are preserved. Mask and Visibility also remember their shape and operation; Slice reuses the current Slice settings.
2. For Mask, choose **Surface brush**, **Rectangle**, **Lasso** or **Line**, then **Add mask** or **Erase mask**. For region shapes, choose **Visible only** to affect the visible surface, or **Through mesh** to include the back and occluded parts. Line also offers **Flip side**. Visibility offers Rectangle, Lasso, Line and Closed path with Hide/Isolate.
3. Hold **Ctrl+Shift** over the model. All brush highlights clear while the hold is active, and brush names stay unchanged. Mask painting still uses the remembered Mask brush preset. The viewport shows the active mode and shape.
4. Paint or draw, release the pen to commit, then release Ctrl+Shift to restore the original brush highlight and sculpt or Modeling tool. **Alt** reverses the operation while held.

The operation strip names the temporary mode, shape and Add/Erase or Hide/Isolate operation while the selection hold is active. Releasing the hold returns its feedback to the previous tool. Your normal brush selection stays intact.

Both tabs put **Invert mask**, **Clear mask**, **Invert visibility** and **Show all** at the top. **Show all** restores the active model’s hidden faces. The popup stays open after each action.

Change **Hold for selection (Mask / visibility)** in **Settings → Key settings** to rebind the shared hold. The optional **Dedicated mask hold** remains available for existing custom bindings.
The **Mask → Hold to mask** section exposes the same choices and a Mask brush preset selector. Surface brushing uses that preset’s pressure, strength, alpha, spacing and stroke settings independently of the ordinary sculpt brush. Surface brushing respects brush symmetry; all shapes protect explicitly hidden geometry. Rectangle, Lasso and Line do not repeat with brush symmetry. These regions work when the gesture begins in empty space and use the chosen depth setting. Existing Lasso/Line presets default to Through mesh.

If you release the hold midway through a freehand stroke, already-painted mask dabs remain as one Undo step. Unfinished rectangle, lasso, line and stamp previews are discarded. Escape or focus loss restores the prior tool; a still-down pen cannot leak into a new sculpt or Modeling gesture. Merely holding, changing the mask mode or opening/dismissing the menu makes no edit.

The shared selection hold follows the selection menu and wins when it overlaps the optional dedicated mask hold. Masking takes priority over Smooth, Reverse, primary-drag zoom/pan and Modeling selection subtraction; hold **Alt** to temporarily reverse Add/Erase; releasing Alt restores the saved operation. Right-drag orbit, middle-drag pan and wheel zoom remain available between gestures. Other action shortcuts pause while masking, except the configured Undo/Redo chords. Key settings reports overlapping assignments; use different keys for the hold and menu invocation. The mode is unavailable during path editing, projection, Slice, transforms, reference arrangement, modal editors or on an unconverted primitive.

## Mask operations

### Example: protect a region with a lasso

1. Choose **Mask → Mask lasso**.
2. Drag an L-shaped outline over the sphere. The green outline closes back to the starting point.
3. Release to apply, then choose **Done**. The dark region is protected.
4. Try a Standard stroke across its boundary. Clay should change on the unmasked side while the dark area stays protected. Undo the trial stroke when finished.

If you protected the wrong side, choose **Invert mask**. To start over, use **Clear mask**; see the controls below.

| While drawing | After release |
| --- | --- |
| ![Green L-shaped mask outline before release](../images/mask-lasso-preview.png) | ![Dark protected region after releasing the mask lasso](../images/mask-lasso-result.png) |

*Mask lasso selects through the model, including its back. Hold Alt to erase masking inside the outline.*

### Protect one side through the model

1. Choose **Mask → Mask through → Line** (also available from **Slice → Mask through · Line**).
2. Choose **Add mask** or **Erase mask** in the tool strip. **Flip side** selects the opposite side of the line.
3. Press in empty viewport space above the model, draw a dividing line across it, and release below it. The tinted side and arrow show the affected half-plane; the line extends beyond both endpoints.
4. Orbit to inspect the back. The same projected side is masked at every depth, in both orthographic and perspective views.

During a drag, **Space** moves the whole line. **Escape** cancels without an edit; **Done** returns to normal sculpting. The existing navigation border and axis gizmo remain reserved for navigation. Away from those controls, an ordinary primary drag belongs to masking even when it starts off the model. Secondary-button orbit, middle-button pan, and your configured zoom/pan holds remain available between gestures. Configured zoom/pan holds take precedence when starting a primary drag, except **Alt**, which reverses Add/Erase.

Line masking changes only mask weights on the active model and all stored subdivision levels. It preserves other masks outside the affected side. Rear-facing surfaces are included; explicitly hidden faces and their boundary vertices stay protected. It never cuts geometry or resets levels. One completed gesture is one Undo step; empty or unchanged selections create no history entry.

**Mask through: Line** and **Flip mask line side** are available in custom menus and **Settings → Key settings**, with no assigned shortcut by default. Assign, remap, or clear them there. The tool strip displays the assigned flip shortcut; it can flip the preview while you hold the drag. Add/Erase and the side checkbox are also available before starting a gesture. **Mask lasso** remains alongside Line for freeform outlines.

### Adjust protection

Use the **Mask** menu:

| Control | Result |
| --- | --- |
| Clear mask | Remove all protection |
| Invert mask | Swap protected and editable regions, including soft values |
| Expand / Contract mask | Grow or shrink by one mesh-edge ring |
| Blur mask | Soften the boundary; repeat for a wider transition |
| Sharpen mask | Increase contrast around 50% |
| Mask intensity | Scale the soft mask's strength |
| Selection threshold | Minimum mask value for grouping, splitting, deletion, and extraction; default 50% |

Blur/sharpen affect every stored subdivision level, including hidden faces. They leave geometry and saved group weights unchanged. Each changed operation is undoable.

Setting intensity to zero clears the mask when the drag ends. Undo or recall a group to restore it. Zero-valued vertices remain excluded even at a zero selection threshold.

### Delete masked geometry

Choose **Delete masked** to remove faces whose vertices all meet the threshold. Other submodels stay intact.

**Deletion can leave open boundaries and resets the active subdivision stack and adaptive/voxel settings.** Undo restores the previous state. Deleting the entire surface leaves an empty model; undo, initialize a primitive, or create a new sphere.

### Modifier taps and drags

**Ctrl-click** (or a pen tip tap with Ctrl held) activates the frontmost visible
submodel in Sculpt, Modeling, Mask, Slice and Transform. Empty space and the
already-active model leave the target unchanged. This uses the same undoable
selection as the Submodels panel and does not modify geometry.

**Ctrl+Shift-click** in Mask inverts soft mask weights; in Visibility it inverts
face visibility. One tap is one Undo/Redo operation. The active temporary
selection context takes priority; Slice, component selection and Transform do
not invert. Menu/popup controls and text entry do not perform viewport taps.

Taps are resolved on release. Moving beyond the normal click tolerance instead
uses the existing drag gesture: Ctrl zoom by default, or the configured
navigation/selection hold bindings. Navigation borders, the view gizmo and edit
handles keep their own interactions. Remapping drag holds does not remap these
explicit Ctrl/Ctrl+Shift tap shortcuts. Escape, focus loss or a disappearing
pointer cancels a pending tap.

### Split the current selection or split to parts

The **Z** menu offers **Split masked**, **Split visible** or **Split selected**
to match its current Mask, Visibility or component-selection context. Submodels
also offers the split for the remembered selection mode, with a separate
**Split to parts** option.

For a direct Modeling face selection, use **Split selected**
in the component-selection Z menu, Modeling controls or Submodels. In Face mode choose **Connected**
and select a face to select its connected part first. The selected face region
moves into one new submodel; an empty or whole-surface selection reports why it
cannot split.

**Split to parts** in **Submodels** separates every vertex-connected surface part
of the active mesh, including hidden faces, into its own submodel. The first
part remains the active source. A mesh with only one part, or insufficient room
under the 64-submodel limit, is unchanged with an explanation.

Both operations leave pieces in place and cut boundaries open. They preserve
masks, groups/weights, visibility, materials, UVs/textures, creases, shading
boundaries and complete quads. They use the current surface and reset the
pieces' subdivision/adaptive/voxel settings; Undo restores the original stack.
Each complete split is one Undo/Redo step and is retained when saving/reopening.
Apply Thickness and convert primitives/array outputs before splitting.

In Visibility context, **Z → Split visible** moves the faces that are not
explicitly hidden into a new submodel. Hidden faces stay hidden in the original.
This is independent of camera occlusion or back-facing faces. Review the face
count and settings reset, then choose **Apply split**; Cancel changes nothing.
Both visible and hidden faces must be present. **Visibility → Split hidden**
remains available for moving the opposite half into a new visible submodel.

### Split masked geometry

1. Paint a mask, and blur it if needed.
2. Open **Mask → Selection threshold**. Lower the threshold to include more of
   a soft mask, or raise it to select only its stronger areas. The count shows
   qualifying triangles. Every corner must meet the threshold; zero mask values
   are excluded even at 0%. The boundary follows existing faces.
3. Choose **Split masked**. Check the model, threshold, face count and settings
   reset, then choose **Apply split**.

The selected faces move into **Masked 1** (or the next free name). The remaining
faces stay in the original active model, and both pieces keep their positions,
mask values, groups, face visibility, paint, UVs, creases and surviving seams.
As with Delete masked and Extract masked, the threshold also selects masked
hidden faces. Use Show all or Clear mask on either piece when needed.

Cut boundaries stay open. Both pieces use the current subdivision level and
reset subdivision, Adaptive and Dynamic Mesh settings. **Cancel** or **Escape**
changes nothing; one **Undo** restores the original. Split requires both selected
and remaining faces, a visible editable model and a free submodel slot. Apply or
remove Thickness first; loose geometry and stale UV layouts must be resolved.

**Extract masked** in Submodels still copies the selected faces while retaining
the source, and clears the copy's mask. **Split masked** moves them and preserves
their mask values. Split masked is also available in search and custom bindings.

## Vertex groups

### Save and recall a region

1. Paint or lasso a mask.
2. Open **Submodels → Vertex groups** and enter a name.
3. Choose **Group masked**.
4. Later, select that group and choose **Mask group** to recall it.

| Group action | Result |
| --- | --- |
| Group all | Replace the visible faces' selection memberships with one group; preserve hidden regions |
| Add masked / Remove masked | Move masked membership into the chosen group / remove it from that group |
| Rename / Delete | Manage a saved group |
| Eye icon | Hide/show complete faces assigned to the group |
| Flip visibility | Swap visible and hidden faces |

Blank names become Group 1, Group 2, and so on. New spheres start in a **Base** group. Group changes are undoable.

**Group all** uses face visibility and group eye visibility inside the active
model. Hidden-only vertices receive no new membership; vertices shared with
visible faces join the new group and keep their old hidden-region weights.
Hidden face labels and visibility stay intact. This applies across stored
subdivision levels and preserves density paint. With no visible faces, the
action is unavailable. A name can be reused only when its previous group is
fully replaced; use a new name if the old group still owns a hidden region.

### Colors and saved weights

Use **View → Show vertex group colors** to check group boundaries, or turn it off to see the clay without group colors.

- Assigning a region to a group replaces its previous membership. Any unassigned remainder stays in its old group; empty groups are removed across stored subdivision levels.
- Neighboring face groups can share boundary vertices. Each face shows one color; a partial face selection can retain weights without coloring the whole face.
- Made the wrong assignment? **Undo** restores the previous groups.

Groups survive saving, duplication, subdivision, extraction, and deletion. Adaptive topology interpolates weights; voxel remeshing approximates boundaries. OBJ exports face groups; `.clay` preserves soft weights.

Thickness can generate front/back/rim groups and masks. These become editable after applying thickness.

## Selection and visibility

### Hide or isolate a region

Choose **Visibility → Select rectangle** or **Select lasso**. Draw the region and release; choose **Done** to return to sculpting.

| During a selection | Effect |
| --- | --- |
| Normal drag | Isolate enclosed faces; green preview |
| Hold Alt | Hide enclosed faces; red preview |
| Hold Space and drag | Move the outline |
| Esc | Cancel the current outline |
| Right drag between selections | Orbit |

Both tools select through all depths on the **active submodel only**. Visibility selects face centers and keeps quads whole. Masks select vertices. Neither repeats with brush symmetry or varies with pressure.

### Hide with a lasso

Use **Select lasso**, hold **Alt**, draw the outline, and release:

| Before release | Hidden region |
| --- | --- |
| ![Red outline previews the region to hide](../images/hide-lasso-preview.png) | ![Faces inside the lasso are hidden, leaving an opening in the displayed sphere](../images/hide-lasso-result.png) |

*Hiding changes visibility, not geometry. Use Show all to restore the view.*

### Line and editable closed path

Choose **Visibility → Hide/Isolate: Line** or **Hide/Isolate: Closed path**. In the tool strip, select **Hide** or **Isolate** and optionally **Flip side / region**. These choices are remembered independently of the sculpt brush. Hold **Alt** to temporarily reverse Hide/Isolate; releasing Alt restores the saved operation.

- **Line:** drag a dividing line, including from empty space. The line extends beyond its endpoints. Release to apply; Space moves the line while dragging.
- **Closed path:** click at least three points around a region. The last point connects visibly to the first. Drag a point to move it, click an edge to insert a point, or use **Delete point**. Up to 64 points are supported. **Apply visibility** commits; **Clear path** discards the outline.
- **Flip side / region:** switches the line half-plane or selects outside the closed region. Crossing paths use alternating inside/outside regions, like the lasso.

Red faces preview exactly the faces that will become hidden. **Isolate** hides currently visible faces outside the selected region; already-hidden faces stay hidden. The classifier uses projected polygon centers at each depth, including perspective projection. Whole quads stay together even when the dividing line crosses them. A center on the boundary belongs to the unflipped region. Hiding never cuts, caps, splits, or moves geometry.

The new shape, operation, flip, Apply, Clear, and Delete point actions are available in **Settings → Key settings** and custom controls. They start unbound; assign, remap, clear, or reset them there. The editor buttons also work without shortcuts.

### Temporary Hide/Isolate

Press **Z**, choose **Visibility**, then choose Rectangle, Lasso, Line, or Closed path and Hide/Isolate. Hold **Ctrl+Shift** to use it. This is the same shared hold used for Mask; change it under **Hold for selection (Mask / visibility)** in Key settings. The chosen shape, operation, and flip state are remembered for the next hold and after restarting.

Release the hold to return to the prior sculpt or Modeling tool without losing its component selection. Keep the hold active until releasing a rectangle, lasso, or line, or applying a multi-click closed path. Releasing the hold first discards the unfinished gesture; it never commits an edit. Escape, focus loss, camera changes, and a change of model discard drafts without a geometry edit. Opening a menu never paints or applies visibility.

For the rectangle shape, the established click behavior remains available:

| Gesture | Result |
| --- | --- |
| Click a group's face | Hide that group; if it was the last visible group, flip visibility |
| Click background | Show all faces |
| Drag entirely in empty background | Invert visibility |

The navigation border, right-button orbit, middle-button pan, and wheel remain available. An unfinished view-space path is discarded when the camera changes.

### Restore visibility and avoid surprises

Use **Visibility → Show all / Invert visibility**. Hidden faces and their shared boundary vertices are protected from brush edits and excluded from picking and wireframe.

Visibility saves with the project and supports undo across stored subdivision levels. Remeshing pauses while faces are hidden; show all before remeshing or slicing.

An unfinished outline is cancelled by tool/view/model changes or loss of focus. Taps and line-only outlines do nothing. Concave lassos work; crossing outlines alternate inside/outside regions. Overlong outlines are cancelled with a message.

### Split hidden faces

Choose **Visibility → Split hidden** to move hidden faces into a new visible
submodel named **Hidden 1**, **Hidden 2**, and so on. Both pieces stay in place.
The dialog identifies the active model and hidden triangle count. Review it,
then choose **Apply split**, or **Cancel** / **Escape** to leave the scene intact.

Both pieces retain masks, group weights and labels, vertex paint/materials,
complete quads, UV coordinates and masks, assigned texture images, crease edges,
UV seam marks and shading seams. The new submodel is shown so it can be edited.
Shared texture images remain shared; splitting does not duplicate their pixels.

You need both visible and hidden faces inside a visible active model and a free
submodel slot (maximum 64). Apply/remove Thickness first. Loose edges or isolated
modeling points and stale UV layouts must be resolved first; unavailable controls
explain the reason. Convert a parametric primitive before using either operation.

Boundaries remain open. The dialog explains that the current subdivision level
is retained, other stored levels are discarded, and subdivision/Adaptive/Dynamic
Mesh settings reset on both pieces. One Undo restores the complete original
model; Redo and saving/reopening preserve the split.

### Delete hidden faces

1. Hide unwanted faces with a Visibility tool or a vertex group's eye control.
2. Choose **Visibility → Delete hidden**, or find **Delete hidden** in tool search.
3. Check the active model and hidden count, then choose **Delete hidden faces**.

This removes the hidden faces inside the active mesh. Other hidden submodels
and faces merely occluded by the camera are not deletion targets. The visible
surface stays in place with the same supported attributes as Split hidden;
shared boundary vertices remain where surviving faces need them. Unused vertices
from the deleted part are removed, and cut boundaries stay open.

The confirmation explains the same subdivision/settings reset. **Cancel** or
**Escape** changes nothing; one **Undo** restores the geometry, attributes and
visibility. No hidden faces means no edit or Undo entry. If every face is hidden,
the dialog warns that deletion will leave an empty editable model in the scene.
Undo restores its surface; use Delete submodel to remove the scene object itself.
Split and Delete hidden can both be assigned shortcuts or placed in custom menus.

## Related guides

[Model tools](modeling.md) · [Undo and redo](sculpting.md#undo-and-redo) · [Thickness modifier](modeling.md#thickness-modifier)


### Quick menus

**Q** always opens Modeling. **Z** opens Select / Mask / Visibility / Slice choices; Select appears while Modeling is enabled. The selected tab decides whether the shared Ctrl+Shift hold selects components, masks, changes visibility or slices. **E** opens transform options for selected components in Modeling or the active object otherwise. All three bindings can be changed or cleared in Key settings.

The chooser changes remembered choices without starting a gesture. Use the shared Ctrl+Shift hold to apply the selected tab’s choices. Surface brush, line and lasso masking support Add/Erase; visibility offers Rectangle, Lasso, Line and Closed path with Hide/Isolate. **Alt reverses the current operation**, including during a drag and when applying a closed path, without changing the shape, flipped side or saved preset. If Alt is itself part of a configured hold, it also reverses the operation; use a different hold to reserve Alt solely for reversal.
