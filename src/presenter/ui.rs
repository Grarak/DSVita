use crate::cartridge_io::{CartridgeIo, CartridgePreview};
use crate::core::graphics::gpu_renderer::GpuRenderer;
use crate::game_info::get_game_info;
use crate::global_settings::{GlobalSettings, MenuLayout};
use crate::key_bindings::KeyBinding;
use crate::presenter::imgui::root::{
    ImDrawData, ImDrawList_AddImage, ImDrawList_AddQuad, ImDrawList_AddQuadFilled, ImDrawList_AddRect, ImDrawList_AddRectFilled, ImDrawList_AddText, ImFontAtlas_AddFontFromMemoryTTF,
    ImFontAtlas_GetGlyphRangesDefault, ImFontConfig, ImFontConfig_ImFontConfig, ImGui, ImGuiCol__ImGuiCol_Button, ImGuiCol__ImGuiCol_ButtonActive, ImGuiCol__ImGuiCol_ButtonHovered, ImGuiCol__ImGuiCol_Text, ImGuiCond__ImGuiSetCond_Always,
    ImGuiHoveredFlags__ImGuiHoveredFlags_Default, ImGuiItemFlags__ImGuiItemFlags_Disabled, ImGuiNavInput__ImGuiNavInput_Cancel, ImGuiNavInput__ImGuiNavInput_FocusNext,
    ImGuiNavInput__ImGuiNavInput_FocusPrev, ImGuiStyleVar__ImGuiStyleVar_Alpha, ImGuiWindowFlags__ImGuiWindowFlags_AlwaysAutoResize, ImGuiWindowFlags__ImGuiWindowFlags_HorizontalScrollbar,
    ImGuiWindowFlags__ImGuiWindowFlags_NoBringToFrontOnFocus, ImGuiWindowFlags__ImGuiWindowFlags_NoCollapse, ImGuiWindowFlags__ImGuiWindowFlags_NoFocusOnAppearing,
    ImGuiWindowFlags__ImGuiWindowFlags_NoMove, ImGuiWindowFlags__ImGuiWindowFlags_NoResize, ImGuiWindowFlags__ImGuiWindowFlags_NoTitleBar, ImVec2, ImVec4,
};
use crate::presenter::{cjk_font, default_key_binding, show_controls_create_settings, show_layout_create_settings, show_retroachievements_settings, PRESENTER_SCREEN_HEIGHT, PRESENTER_SCREEN_WIDTH};
use crate::ra_context::RaContext;
use crate::screen_layouts::{CustomLayout, ScreenLayouts};
use crate::screen_overlays;
use crate::settings::{SettingGroup, SettingValue, Settings, SettingsConfig};
use std::ffi::CString;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::{fs, mem, path, ptr};
use strum::IntoEnumIterator;

pub trait UiBackend {
    fn init(&mut self);
    fn new_frame(&mut self) -> bool;
    fn render_draw_data(&mut self, draw_data: *mut ImDrawData);
    fn swap_window(&mut self);
}
const OVERLAY_FLAGS: u32 =
    (ImGuiWindowFlags__ImGuiWindowFlags_NoTitleBar | ImGuiWindowFlags__ImGuiWindowFlags_NoResize | ImGuiWindowFlags__ImGuiWindowFlags_NoMove | ImGuiWindowFlags__ImGuiWindowFlags_NoCollapse) as u32;

// NoFocusOnAppearing/NoBringToFrontOnFocus: switching the menu layout makes the
// new browse window (##left or ##tiles) first appear while the global-settings
// overlay is open; without these it steals focus from the overlay and draws in
// front of it, leaving the overlay stuck (Back can't close an unfocused overlay).
const PANEL_FLAGS: u32 = OVERLAY_FLAGS | ImGuiWindowFlags__ImGuiWindowFlags_NoBringToFrontOnFocus as u32 | ImGuiWindowFlags__ImGuiWindowFlags_NoFocusOnAppearing as u32;

const RIGHT_PANEL_FLAGS: u32 = PANEL_FLAGS;

#[inline]
unsafe fn begin_window(id: &std::ffi::CStr, x: f32, y: f32, w: f32, h: f32, flags: u32) -> bool {
    let pos = ImVec2 { x, y };
    let pivot = ImVec2 { x: 0.0, y: 0.0 };
    ImGui::SetNextWindowPos(&pos, ImGuiCond__ImGuiSetCond_Always as _, &pivot);
    let sz = ImVec2 { x: w, y: h };
    ImGui::SetNextWindowSize(&sz, ImGuiCond__ImGuiSetCond_Always as _);
    ImGui::Begin(id.as_ptr() as _, ptr::null_mut(), flags as _)
}

#[inline]
unsafe fn begin_fullscreen_overlay(id: &std::ffi::CStr) -> bool {
    begin_window(id, 0.0, 0.0, 960.0, 544.0, OVERLAY_FLAGS)
}

#[inline]
unsafe fn full_width_button(label: &std::ffi::CStr) -> bool {
    let sz = ImVec2 { x: -1f32, y: 0f32 };
    ImGui::Button(label.as_ptr() as _, &sz)
}

#[inline]
unsafe fn icon_image(tex: u32) {
    let sz = ImVec2 { x: 128f32, y: 128f32 };
    let uv = ImVec2 { x: 0f32, y: 0f32 };
    let uv1 = ImVec2 { x: 1f32, y: 1f32 };
    let tint = ImVec4 { x: 1f32, y: 1f32, z: 1f32, w: 1f32 };
    let border = ImVec4 { x: 0f32, y: 0f32, z: 0f32, w: 0f32 };
    ImGui::Image(tex as _, &sz, &uv, &uv1, &tint, &border);
}

#[inline]
unsafe fn nav_input_pressed(input: u32) -> bool {
    // Edge-triggered: imgui sets DownDuration to 0.0 only on the frame the
    // input is first pressed (-1.0 when up, increasing while held). Using the
    // raw NavInputs value instead would fire every frame the input is held.
    (*ImGui::GetIO()).NavInputsDownDuration[input as usize] == 0f32
}

#[inline]
unsafe fn cancel_pressed() -> bool {
    // Edge-triggered so back-navigation doesn't re-trigger once imgui has
    // already stepped out a level while the key is still held.
    nav_input_pressed(ImGuiNavInput__ImGuiNavInput_Cancel)
}

/// Manual tab bar for the settings categories (imgui 1.61 has no TabBar API):
/// a row of equal-width buttons, the active one highlighted. Labels come from
/// `SETTING_GROUPS`. Sets `*active` to the clicked tab index.
unsafe fn settings_tab_bar(active: &mut usize) {
    let spacing = (*ImGui::GetStyle()).ItemSpacing.x;
    let n = SettingGroup::iter().len() as f32;
    let total = ImGui::GetContentRegionAvail().x;
    let w = (total - spacing * (n - 1.0)) / n;
    for (i, group) in SettingGroup::iter().enumerate() {
        if i > 0 {
            ImGui::SameLine(0.0, -1.0);
        }
        let is_active = *active == i;
        if is_active {
            ImGui::PushStyleColor(ImGuiCol__ImGuiCol_Button as _, 0xFFCC8844u32);
        }
        let label = CString::from_str(group.into()).unwrap();
        let sz = ImVec2 { x: w, y: 0.0 };
        if ImGui::Button(label.as_ptr() as _, &sz) {
            *active = i;
        }
        if is_active {
            ImGui::PopStyleColor(1);
        }
    }
}

/// Renders the tabbed settings body: the category tab bar and the active tab's
/// settings (each with its own inline description) in a scroll child.
/// `button_reserve` is extra height to keep free below for a caller button
/// (e.g. Save); 0 if none.
unsafe fn render_settings_tabs(settings_config: &mut SettingsConfig, active_tab: &mut usize, only_runtime: bool, button_reserve: f32) {
    // L / R shoulder buttons cycle through the category tabs. imgui's gamepad
    // backend maps the shoulders to FocusPrev/FocusNext; nothing else consumes
    // them here, so read them directly (edge-triggered).
    let n = SettingGroup::iter().len();
    if nav_input_pressed(ImGuiNavInput__ImGuiNavInput_FocusPrev) {
        *active_tab = (*active_tab + n - 1) % n;
    }
    if nav_input_pressed(ImGuiNavInput__ImGuiNavInput_FocusNext) {
        *active_tab = (*active_tab + 1) % n;
    }

    settings_tab_bar(active_tab);
    ImGui::Separator();

    let child_sz = ImVec2 { x: 0f32, y: -button_reserve };
    if ImGui::BeginChild(c"##settings_scroll".as_ptr() as _, &child_sz, false, 0) {
        // Invisible nav-stops above the first / below the last setting. Gamepad
        // nav only scrolls far enough to reveal the focused item; since a
        // setting's control sits at the bottom of its (taller) description block,
        // focusing the first/last control alone never exposes the very top/bottom
        // of the list. These give nav something to land on past either end.
        nav_scroll_stop(c"##top_stop");
        render_tab_settings(settings_config, SettingGroup::iter().skip(*active_tab).next().unwrap(), only_runtime);
        nav_scroll_stop(c"##bottom_stop");
    }
    ImGui::EndChild();
}

/// A zero-height, full-width invisible button that gamepad nav can focus, used to
/// let nav scroll past the first/last real control to the list's edge.
unsafe fn nav_scroll_stop(id: &std::ffi::CStr) {
    let sz = ImVec2 {
        x: ImGui::GetContentRegionAvail().x.max(1f32),
        y: 1f32,
    };
    ImGui::InvisibleButton(id.as_ptr() as _, &sz);
}

/// True if the cancel/back press should close the current overlay rather than
/// step back a level. imgui processes Cancel in NewFrame *before* our code: if
/// nav was inside a child/popup it already popped one level this frame, so the
/// overlay must only close when it was itself the focused window last frame.
/// Callers pass the previous frame's `IsWindowFocused` measurement.
unsafe fn back_closes_overlay(prev_overlay_focused: bool) -> bool {
    cancel_pressed() && prev_overlay_focused
}
/// One-time global style tweaks for a cleaner, more rounded look.
unsafe fn setup_style() {
    let style = &mut *ImGui::GetStyle();
    style.WindowRounding = 0.0; // fullscreen panels look better square
    style.ChildRounding = 6.0;
    style.FrameRounding = 6.0;
    style.PopupRounding = 8.0;
    style.GrabRounding = 6.0;
    style.ScrollbarRounding = 8.0;
    style.WindowBorderSize = 0.0;
    style.FrameBorderSize = 0.0;
    style.PopupBorderSize = 0.0;
    style.WindowPadding = ImVec2 { x: 10.0, y: 8.0 };
    style.FramePadding = ImVec2 { x: 8.0, y: 4.0 };
    style.ItemSpacing = ImVec2 { x: 7.0, y: 5.0 };
    style.ItemInnerSpacing = ImVec2 { x: 6.0, y: 4.0 };
    style.ScrollbarSize = 12.0;
    style.GrabMinSize = 10.0;
    style.ButtonTextAlign = ImVec2 { x: 0.5, y: 0.5 };
}

/// Horizontally centers `text` within the content region and draws it.
unsafe fn centered_text(text: &std::ffi::CStr) {
    let w = ImGui::CalcTextSize(text.as_ptr(), ptr::null(), false, 0.0).x;
    let avail = ImGui::GetContentRegionAvail().x;
    if avail > w {
        ImGui::SetCursorPosX(ImGui::GetCursorPosX() + (avail - w) * 0.5);
    }
    ImGui::Text(text.as_ptr() as _);
}

/// Large centered heading followed by a separator. Top of a dialog/overlay.
unsafe fn dialog_title(text: &std::ffi::CStr) {
    ImGui::SetWindowFontScale(1.4);
    centered_text(text);
    ImGui::SetWindowFontScale(1.0);
    ImGui::Spacing();
    ImGui::Separator();
    ImGui::Spacing();
}

/// Centered, fixed-width button for vertical menus.
unsafe fn menu_button(label: &std::ffi::CStr, width: f32) -> bool {
    let avail = ImGui::GetContentRegionAvail().x;
    if avail > width {
        ImGui::SetCursorPosX(ImGui::GetCursorPosX() + (avail - width) * 0.5);
    }
    let sz = ImVec2 { x: width, y: 42.0 };
    ImGui::Button(label.as_ptr() as _, &sz)
}

/// Centers the next window on screen — call before `BeginPopupModal`.
unsafe fn center_next_window() {
    let center = ImVec2 { x: 480.0, y: 272.0 };
    let pivot = ImVec2 { x: 0.5, y: 0.5 };
    ImGui::SetNextWindowPos(&center, ImGuiCond__ImGuiSetCond_Always as _, &pivot);
}

/// Directory holding user overlay PNGs (`cartridge_path/overlays`). Set once in
/// `show_main_menu`; read by the overlay picker and preview.
static mut OVERLAYS_DIR: Option<PathBuf> = None;

/// One-entry cache of the GL texture for the currently previewed overlay, keyed
/// by file name so we only re-decode/upload when the selection changes.
struct OverlayPreview {
    name: String,
    tex: u32,
}
static mut OVERLAY_PREVIEW: Option<OverlayPreview> = None;

#[derive(Clone, Copy)]
struct OverlayPreviewInfo {
    tex: u32,
}

pub(crate) fn set_overlays_dir(dir: PathBuf) {
    unsafe {
        *(&raw mut OVERLAYS_DIR) = Some(dir);
    }
}

/// Best-effort absolute form of `path` for showing the user where to drop a
/// file. Canonicalizes the parent dir (the file itself may not exist yet),
/// falling back to the raw path.
fn abs_path_display(path: &Path) -> String {
    match (path.parent(), path.file_name()) {
        (Some(dir), Some(name)) => match fs::canonicalize(dir) {
            Ok(abs) => abs.join(name).display().to_string(),
            Err(_) => path.display().to_string(),
        },
        _ => path.display().to_string(),
    }
}

/// Absolute path of the overlays dir, for telling the user where to drop PNGs.
fn overlays_dir_display() -> String {
    unsafe {
        match (*(&raw const OVERLAYS_DIR)).as_deref() {
            Some(dir) => fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf()).display().to_string(),
            None => String::new(),
        }
    }
}

fn available_overlays() -> Vec<String> {
    unsafe {
        match (*(&raw const OVERLAYS_DIR)).as_deref() {
            Some(dir) => screen_overlays::list(dir),
            None => Vec::new(),
        }
    }
}

/// Loads (and caches) the GL texture for overlay `name`. `None` when the
/// overlay can't be decoded or no overlays dir is set.
unsafe fn overlay_preview(name: &str) -> Option<OverlayPreviewInfo> {
    let dir = (*(&raw const OVERLAYS_DIR)).as_deref()?;

    let cache = &mut *(&raw mut OVERLAY_PREVIEW);
    if let Some(p) = cache {
        if p.name == name {
            return Some(OverlayPreviewInfo { tex: p.tex });
        }
        if p.tex != 0 {
            gl::DeleteTextures(1, &p.tex);
        }
        *cache = None;
    }

    let overlay = screen_overlays::load(&dir.join(name))?;

    let mut tex = 0;
    gl::GenTextures(1, &mut tex);
    gl::BindTexture(gl::TEXTURE_2D, tex);
    gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::LINEAR as _);
    gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::LINEAR as _);
    gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as _);
    gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as _);
    gl::TexImage2D(
        gl::TEXTURE_2D,
        0,
        gl::RGBA as _,
        overlay.width as _,
        overlay.height as _,
        0,
        gl::RGBA,
        gl::UNSIGNED_BYTE,
        overlay.data.as_ptr() as _,
    );

    *cache = Some(OverlayPreview { name: name.to_string(), tex });
    Some(OverlayPreviewInfo { tex })
}

/// Overlay picker combo ("None" + every `.png` in the overlays dir). Shared by
/// both platforms' layout editors.
pub(crate) unsafe fn draw_overlay_picker(custom_layout: &mut CustomLayout) {
    ImGui::Spacing();
    ImGui::Separator();
    ImGui::TextDisabled(c"Overlay".as_ptr());
    let hint = CString::new(format!("Place .png files in {}", overlays_dir_display())).unwrap();
    ImGui::TextDisabled(hint.as_ptr());

    let overlays = available_overlays();
    let preview = CString::new(custom_layout.overlay.as_deref().unwrap_or("None")).unwrap();
    let none_selected = custom_layout.overlay.is_none();
    let sz = ImVec2 { x: 0f32, y: 0f32 };

    if ImGui::BeginCombo(c"##overlay".as_ptr(), preview.as_ptr(), 0) {
        if ImGui::Selectable(c"None".as_ptr(), none_selected, 0, &sz) {
            custom_layout.overlay = None;
        }
        if none_selected {
            ImGui::SetItemDefaultFocus();
        }
        for name in &overlays {
            let is_selected = custom_layout.overlay.as_deref() == Some(name.as_str());
            let label = CString::new(name.as_str()).unwrap();
            if ImGui::Selectable(label.as_ptr(), is_selected, 0, &sz) {
                custom_layout.overlay = Some(name.clone());
            }
            if is_selected {
                ImGui::SetItemDefaultFocus();
            }
        }
        ImGui::EndCombo();
    }
}

/// Draws a scaled preview of the presenter screen with the two DS screens
/// positioned/sized/rotated exactly as the layout would render them. Reserves
/// the canvas area via a Dummy so surrounding layout flows normally.
pub(crate) unsafe fn draw_layout_preview(custom_layout: &CustomLayout) {
    let dl = ImGui::GetWindowDrawList();
    let origin = ImGui::GetCursorScreenPos();
    let avail = ImGui::GetContentRegionAvail();

    let scale = (avail.x / PRESENTER_SCREEN_WIDTH as f32).min(avail.y / PRESENTER_SCREEN_HEIGHT as f32).max(0.0);
    let box_w = PRESENTER_SCREEN_WIDTH as f32 * scale;
    let box_h = PRESENTER_SCREEN_HEIGHT as f32 * scale;
    let ox = origin.x + (avail.x - box_w) * 0.5;
    let oy = origin.y;

    let bg_min = ImVec2 { x: ox, y: oy };
    let bg_max = ImVec2 { x: ox + box_w, y: oy + box_h };
    // rounding = 0 so the corner-flags arg (whose meaning differs across imgui
    // versions: Vita SDK vs the bundled 1.61) doesn't matter.
    ImDrawList_AddRectFilled(dl, &bg_min, &bg_max, 0xFF1A1A1A, 0.0, 0);
    ImDrawList_AddRect(dl, &bg_min, &bg_max, 0xFF505050, 0.0, 0, 1.0);

    // Selected overlay stretched over the whole preview box, behind the screen
    // quads — mirrors how the renderer draws it under the merged screens.
    if let Some(name) = custom_layout.overlay.as_deref() {
        if let Some(info) = overlay_preview(name) {
            if info.tex != 0 {
                let uv0 = ImVec2 { x: 0.0, y: 0.0 };
                let uv1 = ImVec2 { x: 1.0, y: 1.0 };
                ImDrawList_AddImage(dl, info.tex as _, &bg_min, &bg_max, &uv0, &uv1, 0xFFFFFFFF);
            }
        }
    }

    const FILL: [u32; 2] = [0xCCC8783C, 0xCC3C78C8]; // top: blue-ish, bottom: orange-ish (0xAABBGGRR)
    const OUTLINE: [u32; 2] = [0xFFE89858, 0xFF58A8E8];
    const LABELS: [&std::ffi::CStr; 2] = [c"Top", c"Bottom"];

    for i in 0..2 {
        let corners = custom_layout.screen_corners(i);
        let p: [ImVec2; 4] = [
            ImVec2 {
                x: ox + corners[0].0 * scale,
                y: oy + corners[0].1 * scale,
            },
            ImVec2 {
                x: ox + corners[1].0 * scale,
                y: oy + corners[1].1 * scale,
            },
            ImVec2 {
                x: ox + corners[2].0 * scale,
                y: oy + corners[2].1 * scale,
            },
            ImVec2 {
                x: ox + corners[3].0 * scale,
                y: oy + corners[3].1 * scale,
            },
        ];
        ImDrawList_AddQuadFilled(dl, &p[0], &p[1], &p[2], &p[3], FILL[i]);
        ImDrawList_AddQuad(dl, &p[0], &p[1], &p[2], &p[3], OUTLINE[i], 1.5);

        let center = ImVec2 {
            x: (p[0].x + p[1].x + p[2].x + p[3].x) * 0.25,
            y: (p[0].y + p[1].y + p[2].y + p[3].y) * 0.25,
        };
        let label = LABELS[i];
        let ts = ImGui::CalcTextSize(label.as_ptr(), ptr::null(), false, 0.0);
        let text_pos = ImVec2 {
            x: center.x - ts.x * 0.5,
            y: center.y - ts.y * 0.5,
        };
        ImDrawList_AddText(dl, &text_pos, 0xFFFFFFFF, label.as_ptr(), ptr::null());
    }

    let reserve = ImVec2 { x: avail.x, y: box_h };
    ImGui::Dummy(&reserve);
}

/// Dim, centered "go back" hint pinned to the bottom of a fullscreen overlay.
unsafe fn back_hint() {
    let target = ImGui::GetWindowHeight() - ImGui::GetFrameHeightWithSpacing();
    if target > ImGui::GetCursorPosY() {
        ImGui::SetCursorPosY(target);
    }
    let text = c"Press Circle to go back";
    let w = ImGui::CalcTextSize(text.as_ptr(), ptr::null(), false, 0.0).x;
    let avail = ImGui::GetContentRegionAvail().x;
    if avail > w {
        ImGui::SetCursorPosX(ImGui::GetCursorPosX() + (avail - w) * 0.5);
    }
    ImGui::TextDisabled(text.as_ptr() as _);
}
pub fn init_ui(ui_backend: &mut impl UiBackend) {
    unsafe {
        ImGui::CreateContext(ptr::null_mut());
        ImGui::StyleColorsDark(ptr::null_mut());
        setup_style();
        ui_backend.init();

        let font = include_bytes!("../../font/OpenSans-Regular.ttf");
        let mut config: ImFontConfig = mem::zeroed();
        ImFontConfig_ImFontConfig(&mut config);
        config.FontDataOwnedByAtlas = false;
        ImFontAtlas_AddFontFromMemoryTTF(
            (*ImGui::GetIO()).Fonts,
            font.as_ptr() as _,
            font.len() as _,
            22f32,
            &config,
            ImFontAtlas_GetGlyphRangesDefault((*ImGui::GetIO()).Fonts),
        );
    }
}
/// Renders one setting: the title, its description wrapped on the left, and the
/// control bottom-aligned to the right of the description, then a separator.
///
/// The control sits at the bottom of the description (rather than the description
/// being pinned in a footer) so gamepad nav, which only scrolls far enough to
/// reveal the focused control, always brings the whole description into view too.
unsafe fn render_setting(setting: &mut crate::settings::Setting, id: usize, dirty: &mut bool, buttons_width: f32) {
    const COMBO_WIDTH: f32 = 200f32;

    let title = CString::new(setting.title).unwrap();
    ImGui::Text(title.as_ptr() as _);

    let style = &*ImGui::GetStyle();
    let control_w = match setting.value {
        SettingValue::Bool(_) => buttons_width,
        SettingValue::List(_) => COMBO_WIDTH,
        SettingValue::Int(_) => COMBO_WIDTH,
        SettingValue::Slider(_) => COMBO_WIDTH,
    };
    let region_x = ImGui::GetCursorPosX();
    let avail = ImGui::GetContentRegionAvail().x;
    let desc_top = ImGui::GetCursorPosY();

    // Description on the left, wrapped so it never runs under the control column.
    if !setting.description.is_empty() {
        let wrap = region_x + (avail - control_w - style.ItemSpacing.x).max(1f32);
        ImGui::PushTextWrapPos(wrap);
        let description = CString::new(setting.description).unwrap();
        ImGui::TextDisabled(description.as_ptr() as _);
        ImGui::PopTextWrapPos();
    }
    let desc_bottom = ImGui::GetCursorPosY();

    // Bottom-align the control to the last line of the description.
    let control_h = ImGui::GetFrameHeight();
    let control_y = (desc_bottom - control_h).max(desc_top);
    ImGui::SetCursorPosX(region_x + avail - control_w);
    ImGui::SetCursorPosY(control_y);

    ImGui::PushID3(id as _);
    match &mut setting.value {
        SettingValue::Bool(_) => {
            let value = CString::new(setting.value.to_string()).unwrap();
            let sz = ImVec2 { x: control_w, y: 0f32 };
            if ImGui::Button(value.as_ptr() as _, &sz) {
                setting.value.next();
                *dirty = true;
            }
        }
        SettingValue::List(inner) => {
            if inner.selection >= inner.values.len() {
                inner.selection = 0;
            }
            let value = CString::from_str(&inner.values[inner.selection]).unwrap();
            let id = CString::new(format!("##{id}_list")).unwrap();
            // Constrain the combo to control_w so it doesn't overrun the window
            // edge (which would make the window horizontally scrollable).
            ImGui::PushItemWidth(control_w);
            if ImGui::BeginCombo(id.as_ptr() as _, value.as_ptr() as _, 0) {
                for (j, val) in inner.values.iter().enumerate() {
                    let is_selected = j == inner.selection;
                    let val_cstr = CString::from_str(val).unwrap();
                    let sz = ImVec2 { x: 0f32, y: 0f32 };
                    if ImGui::Selectable(val_cstr.as_ptr() as _, is_selected, 0, &sz) {
                        inner.selection = j;
                        *dirty = true;
                    }
                    if is_selected {
                        ImGui::SetItemDefaultFocus();
                    }
                }
                ImGui::EndCombo();
            }
            ImGui::PopItemWidth();
        }
        SettingValue::Int(_) => {}
        SettingValue::Slider(inner) => {
            let id = CString::new(format!("##{id}_slider")).unwrap();
            ImGui::PushItemWidth(control_w);
            let mut value = inner.value;
            if ImGui::SliderInt(id.as_ptr() as _, &mut value, inner.min, inner.max, c"%d".as_ptr()) {
                inner.value = value.clamp(inner.min, inner.max);
                *dirty = true;
            }
            ImGui::PopItemWidth();
        }
    }
    ImGui::PopID();

    // Drop below whichever of description / control reaches lower, then divide.
    ImGui::SetCursorPosY(desc_bottom.max(control_y + control_h));
    ImGui::Spacing();
    ImGui::Separator();
    ImGui::Spacing();
}

/// Renders the active tab's settings.
unsafe fn render_tab_settings(settings_config: &mut SettingsConfig, group: SettingGroup, only_runtime: bool) {
    let all = settings_config.settings.get_all_mut();
    for (i, setting) in all.iter_mut().enumerate() {
        if setting.group != group || (only_runtime && !setting.runtime) {
            continue;
        }
        render_setting(setting, i, &mut settings_config.dirty, 50f32);
    }
}
unsafe fn render_global_settings_overlay(
    show: &mut bool,
    global_settings: &mut GlobalSettings,
    layout_settings: &mut bool,
    controls_settings: &mut bool,
    ra_settings: &mut bool,
    cjk_download: &cjk_font::Download,
    cjk_applied: bool,
    overlay_focused: &mut bool,
) {
    if !*show {
        *overlay_focused = true;
        return;
    }
    if !begin_fullscreen_overlay(c"##globalsettings") {
        ImGui::End();
        return;
    }
    dialog_title(c"Global Settings");

    const BUTTON_WIDTH: f32 = 380.0;
    if menu_button(global_settings.menu_layout.label(), BUTTON_WIDTH) {
        global_settings.set_menu_layout(global_settings.menu_layout.next());
    }
    if menu_button(c"Custom screen layout", BUTTON_WIDTH) {
        *layout_settings = true;
    }
    if menu_button(c"Custom controls", BUTTON_WIDTH) {
        *controls_settings = true;
    }
    if menu_button(c"RetroAchievements", BUTTON_WIDTH) {
        *ra_settings = true;
    }

    ImGui::Spacing();
    ImGui::Separator();
    ImGui::Spacing();
    let (done, downloading, error) = cjk_download.snapshot();
    match downloading {
        None => {
            if cjk_applied {
                centered_text(c"Chinese font installed.");
            } else if done {
                centered_text(c"Chinese font downloaded. Restart to apply.");
            } else {
                centered_text(c"Download a font to show Chinese game names.");
                if menu_button(c"Download Chinese font", BUTTON_WIDTH) {
                    cjk_download.start();
                }
                // Manual fallback for when the automatic download is blocked
                // (e.g. Gitee refusing anonymous hot-links).
                ImGui::Spacing();
                let hint = CString::new(format!("If the download fails, put wqy-microhei.ttc here manually:\n{}", abs_path_display(&cjk_download.path))).unwrap();
                ImGui::PushTextWrapPos(0.0);
                ImGui::TextDisabled(hint.as_ptr());
                ImGui::PopTextWrapPos();
            }
        }
        Some((current_len, total_len)) => {
            centered_text(c"Downloading Chinese font...");
            ImGui::Spacing();
            let fraction = if total_len == 0 { 0.0 } else { current_len as f32 / total_len as f32 };
            let overlay = CString::new(format!("{} KB", current_len / 1024)).unwrap();
            let avail = ImGui::GetContentRegionAvail().x;
            if avail > BUTTON_WIDTH {
                ImGui::SetCursorPosX(ImGui::GetCursorPosX() + (avail - BUTTON_WIDTH) * 0.5);
            }
            let sz = ImVec2 { x: BUTTON_WIDTH, y: 0.0 };
            ImGui::ProgressBar(fraction, &sz, overlay.as_ptr());
        }
    }
    if !error.is_empty() {
        ImGui::PushStyleColor(ImGuiCol__ImGuiCol_Text as _, 0xFF0000FFu32);
        if let Ok(error) = CString::new(error) {
            centered_text(&error);
        }
        ImGui::PopStyleColor(1);
    }

    back_hint();

    // Only close when this overlay itself is focused (not a sub-overlay stacked
    // on top), so Back steps down one level instead of closing the whole stack.
    if back_closes_overlay(*overlay_focused) {
        *show = false;
    }
    *overlay_focused = ImGui::IsWindowFocused(0);
    ImGui::End();
}

unsafe fn render_layout_settings_overlay(
    show: &mut bool,
    global_settings: &mut GlobalSettings,
    screen_layouts: &mut ScreenLayouts,
    custom_layout: &mut bool,
    custom_layout_context: &mut CustomLayoutContext,
    new_custom_layout: &mut CustomLayout,
    selected_custom_layout: &mut Option<usize>,
    overlay_focused: &mut bool,
) {
    if !*show {
        *overlay_focused = true;
        return;
    }
    if !begin_fullscreen_overlay(c"##layoutsettings") {
        ImGui::End();
        return;
    }
    dialog_title(c"Custom Screen Layouts");

    center_next_window();
    if ImGui::BeginPopupModal(
        c"customlayoutmenu".as_ptr(),
        ptr::null_mut(),
        (ImGuiWindowFlags__ImGuiWindowFlags_NoTitleBar
            | ImGuiWindowFlags__ImGuiWindowFlags_NoResize
            | ImGuiWindowFlags__ImGuiWindowFlags_NoMove
            | ImGuiWindowFlags__ImGuiWindowFlags_NoCollapse
            | ImGuiWindowFlags__ImGuiWindowFlags_AlwaysAutoResize) as _,
    ) {
        let bsz = ImVec2 { x: 120.0, y: 44.0 };
        if ImGui::Button(c"Delete".as_ptr(), &bsz) {
            global_settings.delete_custom_layout(selected_custom_layout.unwrap());
            screen_layouts.populate_custom_layouts(&global_settings.custom_layouts);
            *selected_custom_layout = None;
            ImGui::CloseCurrentPopup();
        }
        ImGui::SameLine(0.0, (*ImGui::GetStyle()).ItemSpacing.x);
        if ImGui::Button(c"Back".as_ptr(), &bsz) {
            *selected_custom_layout = None;
            ImGui::CloseCurrentPopup();
        }
        ImGui::EndPopup();
    }

    if selected_custom_layout.is_some() {
        ImGui::OpenPopup(c"customlayoutmenu".as_ptr());
    }

    const BUTTON_WIDTH: f32 = 380.0;
    if global_settings.custom_layouts.is_empty() {
        centered_text(c"No custom layouts yet.");
        ImGui::Spacing();
    }
    for (i, layout) in global_settings.custom_layouts.iter().enumerate() {
        let name = layout.name_c_str();
        if menu_button(&name, BUTTON_WIDTH) {
            *selected_custom_layout = Some(i);
        }
    }

    if menu_button(c"Add custom layout", BUTTON_WIDTH) {
        *custom_layout = true;
        *custom_layout_context = CustomLayoutContext::default();
        *new_custom_layout = CustomLayout::default();
    }

    back_hint();

    // A combo/popup/child step-out is handled by imgui; only close this overlay
    // (down to global settings) when it is itself the focused window.
    if back_closes_overlay(*overlay_focused) {
        *show = false;
    }
    *overlay_focused = ImGui::IsWindowFocused(0);
    ImGui::End();
}

unsafe fn render_custom_layout_overlay(
    show: &mut bool,
    global_settings: &mut GlobalSettings,
    screen_layouts: &mut ScreenLayouts,
    custom_layout_context: &mut CustomLayoutContext,
    new_custom_layout: &mut CustomLayout,
    overlay_focused: &mut bool,
) {
    if !*show {
        *overlay_focused = true;
        return;
    }
    if !begin_fullscreen_overlay(c"##createcustomlayout") {
        ImGui::End();
        return;
    }
    dialog_title(c"New Custom Layout");
    if show_layout_create_settings(global_settings, custom_layout_context, new_custom_layout) {
        *show = false;
        screen_layouts.populate_custom_layouts(&global_settings.custom_layouts);
    }
    if back_closes_overlay(*overlay_focused) {
        *show = false;
    }
    *overlay_focused = ImGui::IsWindowFocused(0);
    ImGui::End();
}

unsafe fn render_controls_settings_overlay(
    show: &mut bool,
    global_settings: &mut GlobalSettings,
    custom_controls: &mut bool,
    custom_layout_context: &mut CustomLayoutContext,
    new_key_binding: &mut KeyBinding,
    selected_custom_control: &mut Option<usize>,
    overlay_focused: &mut bool,
) {
    if !*show {
        *overlay_focused = true;
        return;
    }
    if !begin_fullscreen_overlay(c"##controlssettings") {
        ImGui::End();
        return;
    }
    dialog_title(c"Custom Controls");

    center_next_window();
    if ImGui::BeginPopupModal(
        c"customcontrolsmenu".as_ptr(),
        ptr::null_mut(),
        (ImGuiWindowFlags__ImGuiWindowFlags_NoTitleBar
            | ImGuiWindowFlags__ImGuiWindowFlags_NoResize
            | ImGuiWindowFlags__ImGuiWindowFlags_NoMove
            | ImGuiWindowFlags__ImGuiWindowFlags_NoCollapse
            | ImGuiWindowFlags__ImGuiWindowFlags_AlwaysAutoResize) as _,
    ) {
        let bsz = ImVec2 { x: 120.0, y: 44.0 };
        if ImGui::Button(c"Delete".as_ptr(), &bsz) {
            global_settings.delete_custom_controls(selected_custom_control.unwrap());
            *selected_custom_control = None;
            ImGui::CloseCurrentPopup();
        }
        ImGui::SameLine(0.0, (*ImGui::GetStyle()).ItemSpacing.x);
        if ImGui::Button(c"Back".as_ptr(), &bsz) {
            *selected_custom_control = None;
            ImGui::CloseCurrentPopup();
        }
        ImGui::EndPopup();
    }

    if selected_custom_control.is_some() {
        ImGui::OpenPopup(c"customcontrolsmenu".as_ptr());
    }

    const BUTTON_WIDTH: f32 = 380.0;
    if global_settings.custom_controls.is_empty() {
        centered_text(c"No custom controls yet.");
        ImGui::Spacing();
    }
    for (i, binding) in global_settings.custom_controls.iter().enumerate() {
        let name = binding.name_c_str();
        if menu_button(&name, BUTTON_WIDTH) {
            *selected_custom_control = Some(i);
        }
    }

    if menu_button(c"Add custom controls", BUTTON_WIDTH) {
        *custom_controls = true;
        *custom_layout_context = CustomLayoutContext::default();
        *new_key_binding = default_key_binding();
    }

    back_hint();

    if back_closes_overlay(*overlay_focused) {
        *show = false;
    }
    *overlay_focused = ImGui::IsWindowFocused(0);
    ImGui::End();
}

unsafe fn render_custom_controls_overlay(
    show: &mut bool,
    global_settings: &mut GlobalSettings,
    custom_layout_context: &mut CustomLayoutContext,
    new_key_binding: &mut KeyBinding,
    overlay_focused: &mut bool,
) {
    if !*show {
        *overlay_focused = true;
        return;
    }
    if !begin_fullscreen_overlay(c"##createcustomcontrols") {
        ImGui::End();
        return;
    }
    dialog_title(c"New Controls Profile");
    if show_controls_create_settings(global_settings, custom_layout_context, new_key_binding) {
        *show = false;
    }
    if back_closes_overlay(*overlay_focused) {
        *show = false;
    }
    *overlay_focused = ImGui::IsWindowFocused(0);
    ImGui::End();
}

unsafe fn render_ra_settings_overlay(show: &mut bool, global_settings: &mut GlobalSettings, ra_context: &mut RaContext, ra_login_context: &mut RALoginContext, overlay_focused: &mut bool) {
    if !*show {
        *overlay_focused = true;
        return;
    }
    if !begin_fullscreen_overlay(c"##retroachievements") {
        ImGui::End();
        return;
    }
    dialog_title(c"RetroAchievements");
    if ra_login_context.logging_in {
        center_next_window();
        if ImGui::BeginPopupModal(
            c"retroachievements_login_dialog".as_ptr(),
            ptr::null_mut(),
            (ImGuiWindowFlags__ImGuiWindowFlags_NoTitleBar
                | ImGuiWindowFlags__ImGuiWindowFlags_NoResize
                | ImGuiWindowFlags__ImGuiWindowFlags_NoMove
                | ImGuiWindowFlags__ImGuiWindowFlags_NoCollapse
                | ImGuiWindowFlags__ImGuiWindowFlags_AlwaysAutoResize) as _,
        ) {
            centered_text(c"Logging in...");
            ImGui::EndPopup();
        }
        ImGui::OpenPopup(c"retroachievements_login_dialog".as_ptr());
    }
    show_retroachievements_settings(global_settings, ra_login_context, ra_context);
    back_hint();
    if back_closes_overlay(*overlay_focused) {
        *show = false;
    }
    *overlay_focused = ImGui::IsWindowFocused(0);
    ImGui::End();
}
#[derive(Default)]
pub struct CustomLayoutContext {
    pub parse_error: bool,
    pub empty_name: bool,
    pub duplicated_name: bool,
}

#[derive(Default)]
pub struct RALoginContext {
    pub username: String,
    pub password: String,
    pub error: String,
    pub logging_in: bool,
}

pub fn show_main_menu(
    cartridge_path: PathBuf,
    screen_layouts: &mut ScreenLayouts,
    ra_context: &mut RaContext,
    default_keybinding: KeyBinding,
    cjk_download: &mut cjk_font::Download,
    ui_backend: &mut impl UiBackend,
) -> Option<(CartridgeIo, GlobalSettings, Settings, PathBuf)> {
    unsafe {
        let saves_path = cartridge_path.join("saves");
        let global_settings_path = cartridge_path.join("global_settings");
        let settings_path = cartridge_path.join("settings");
        let overlays_path = cartridge_path.join("overlays");
        let _ = fs::create_dir_all(&cartridge_path);
        let _ = fs::create_dir_all(&saves_path);
        let _ = fs::create_dir_all(&global_settings_path);
        let _ = fs::create_dir_all(&settings_path);
        let _ = fs::create_dir_all(&overlays_path);

        let mut global_settings = GlobalSettings::new(global_settings_path.clone(), default_keybinding).unwrap();
        screen_layouts.populate_custom_layouts(&global_settings.custom_layouts);
        screen_layouts.set_overlays_dir(overlays_path.clone());
        set_overlays_dir(overlays_path);
        ra_context.set_cache_dir(cartridge_path.join("ra"));
        cjk_download.set_file_path(&cjk_font::font_path(&cartridge_path));

        let cartridges = load_cartridges(&cartridge_path);
        let mut settings_configs: Vec<SettingsConfig> = cartridges.iter().map(|c| SettingsConfig::new(settings_path.join(format!("{}.ini", c.file_name)))).collect();

        let mut show_global_settings = false;
        let mut hovered: Option<usize> = None;
        let mut detail_game: Option<usize> = None;
        let mut active_tab: usize = 0;
        let mut detail_overlay_focused = true;
        let mut launched = false;

        // Back-nav focus tracking for the stacked global-settings overlays, so
        // Back closes only the topmost one (see render_game_detail_overlay).
        let mut global_settings_focused = true;
        let mut layout_settings = false;
        let mut layout_settings_focused = true;
        let mut custom_layout_show = false;
        let mut custom_layout_focused = true;
        let mut custom_layout_context = CustomLayoutContext::default();
        let mut new_custom_layout = CustomLayout::default();
        let mut selected_custom_layout: Option<usize> = None;

        let mut controls_settings = false;
        let mut controls_settings_focused = true;
        let mut custom_controls_show = false;
        let mut custom_controls_focused = true;
        let mut new_key_binding = KeyBinding::default();
        let mut selected_custom_control: Option<usize> = None;

        let mut ra_settings = false;
        let mut ra_settings_focused = true;
        let mut ra_login_context = RALoginContext::default();

        let cjk_applied = cjk_font::load_once(&cjk_download.path, || {
            let mut texts: Vec<String> = cartridges.iter().map(|c| c.file_name.clone()).collect();
            texts.extend(cartridges.iter().filter_map(|c| c.read_title().ok()));
            texts
        });

        let mut icon_tex = 0;
        gl::GenTextures(1, &mut icon_tex);
        gl::BindTexture(gl::TEXTURE_2D, icon_tex);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as _);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as _);
        gl::TexImage2D(gl::TEXTURE_2D, 0, gl::RGBA as _, 32, 32, 0, gl::RGBA, gl::UNSIGNED_BYTE, ptr::null());

        // One texture per cartridge for the tiled menu, each loaded once up front
        // (the list menu only needs the single `icon_tex` for the hovered game).
        let mut tile_textures = vec![0u32; cartridges.len()];
        if !cartridges.is_empty() {
            gl::GenTextures(tile_textures.len() as _, tile_textures.as_mut_ptr());
            for (texture, cartridge) in tile_textures.iter().zip(cartridges.iter()) {
                gl::BindTexture(gl::TEXTURE_2D, *texture);
                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as _);
                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as _);
                gl::TexImage2D(gl::TEXTURE_2D, 0, gl::RGBA as _, 32, 32, 0, gl::RGBA, gl::UNSIGNED_BYTE, ptr::null());
                load_icon_texture(*texture, cartridge);
            }
        }

        while !launched {
            if !ui_backend.new_frame() {
                return None;
            }

            if let Some(i) = detail_game.or(hovered) {
                load_icon_texture(icon_tex, &cartridges[i]);
            }

            render_menu_bar(&cartridges, &cartridge_path);

            match global_settings.menu_layout {
                MenuLayout::List => {
                    render_left_panel(&cartridges, &mut hovered, &mut show_global_settings, &mut detail_game, &mut active_tab);
                    render_right_panel(&cartridges, icon_tex, hovered);
                }
                MenuLayout::Tiles => {
                    render_tile_grid(&cartridges, &tile_textures, &mut hovered, &mut show_global_settings, &mut detail_game, &mut active_tab);
                }
                MenuLayout::Carousel => {
                    render_carousel(&cartridges, &tile_textures, &mut hovered, &mut show_global_settings, &mut detail_game, &mut active_tab);
                }
            }

            render_game_detail_overlay(
                &cartridges,
                &mut settings_configs,
                screen_layouts,
                &global_settings,
                icon_tex,
                &mut detail_game,
                &mut active_tab,
                &mut detail_overlay_focused,
                &mut launched,
            );

            render_global_settings_overlay(
                &mut show_global_settings,
                &mut global_settings,
                &mut layout_settings,
                &mut controls_settings,
                &mut ra_settings,
                &cjk_download,
                cjk_applied,
                &mut global_settings_focused,
            );
            render_layout_settings_overlay(
                &mut layout_settings,
                &mut global_settings,
                screen_layouts,
                &mut custom_layout_show,
                &mut custom_layout_context,
                &mut new_custom_layout,
                &mut selected_custom_layout,
                &mut layout_settings_focused,
            );
            render_custom_layout_overlay(
                &mut custom_layout_show,
                &mut global_settings,
                screen_layouts,
                &mut custom_layout_context,
                &mut new_custom_layout,
                &mut custom_layout_focused,
            );
            render_controls_settings_overlay(
                &mut controls_settings,
                &mut global_settings,
                &mut custom_controls_show,
                &mut custom_layout_context,
                &mut new_key_binding,
                &mut selected_custom_control,
                &mut controls_settings_focused,
            );
            render_custom_controls_overlay(
                &mut custom_controls_show,
                &mut global_settings,
                &mut custom_layout_context,
                &mut new_key_binding,
                &mut custom_controls_focused,
            );
            render_ra_settings_overlay(&mut ra_settings, &mut global_settings, ra_context, &mut ra_login_context, &mut ra_settings_focused);

            let io = ImGui::GetIO();
            gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
            gl::Viewport(0, 0, (*io).DisplaySize.x as _, (*io).DisplaySize.y as _);
            gl::ClearColor(0.0, 0.0, 0.0, 1.0);
            gl::Clear(gl::COLOR_BUFFER_BIT);
            ImGui::Render();

            ui_backend.render_draw_data(ImGui::GetDrawData());
            ui_backend.swap_window();
        }

        gl::DeleteTextures(1, &icon_tex);
        if !tile_textures.is_empty() {
            gl::DeleteTextures(tile_textures.len() as _, tile_textures.as_ptr());
        }

        let sel = detail_game.unwrap();
        let preview = cartridges.into_iter().nth(sel).unwrap();
        let save_file = saves_path.join(format!("{}.sav", preview.file_name));
        let config = settings_configs.swap_remove(sel);
        Some((CartridgeIo::from_preview(preview, save_file).unwrap(), global_settings, config.settings, config.settings_file_path))
    }
}
unsafe fn load_cartridges(path: &std::path::Path) -> Vec<CartridgePreview> {
    let mut cartridges: Vec<CartridgePreview> = match fs::read_dir(path) {
        Ok(rom_dir) => rom_dir
            .into_iter()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_type().map_or(false, |ft| ft.is_file()))
            .filter_map(|entry| {
                let path = entry.path();
                let name = path.file_name()?.to_str()?;
                if name.to_lowercase().ends_with(".nds") {
                    CartridgePreview::new(path).ok()
                } else {
                    None
                }
            })
            .collect(),
        Err(_) => Vec::new(),
    };
    cartridges.sort_by(|a, b| a.file_name.cmp(&b.file_name));
    cartridges
}

unsafe fn load_icon_texture(tex: u32, cartridge: &CartridgePreview) {
    gl::BindTexture(gl::TEXTURE_2D, tex);
    match cartridge.read_icon() {
        Ok(icon) => gl::TexSubImage2D(gl::TEXTURE_2D, 0, 0, 0, 32, 32, gl::RGBA as _, gl::UNSIGNED_BYTE, icon.as_ptr() as _),
        Err(_) => {
            const EMPTY: [u32; 32 * 32] = [0u32; 32 * 32];
            gl::TexSubImage2D(gl::TEXTURE_2D, 0, 0, 0, 32, 32, gl::RGBA as _, gl::UNSIGNED_BYTE, EMPTY.as_ptr() as _);
        }
    }
}

unsafe fn render_menu_bar(cartridges: &[CartridgePreview], cartridge_path: &std::path::Path) {
    if !ImGui::BeginMainMenuBar() {
        return;
    }
    let text = if cartridges.is_empty() {
        format!("No roms found in {}", cartridge_path.to_str().unwrap())
    } else {
        format!("Found {} roms in {}", cartridges.len(), cartridge_path.to_str().unwrap())
    };
    let text = CString::from_str(&text).unwrap();
    ImGui::Text(text.as_ptr() as _);
    ImGui::EndMainMenuBar();
}

unsafe fn render_left_panel(cartridges: &[CartridgePreview], hovered: &mut Option<usize>, show_global_settings: &mut bool, detail_game: &mut Option<usize>, active_tab: &mut usize) {
    // Sit directly below the main menu bar (its height tracks FramePadding).
    let top = ImGui::GetFrameHeight();
    if !begin_window(c"##left", 0.0, top, 700.0, 544.0 - top, PANEL_FLAGS) {
        ImGui::End();
        return;
    }
    if full_width_button(c"Global settings") {
        *show_global_settings = true;
    }
    if ImGui::IsItemHovered(ImGuiHoveredFlags__ImGuiHoveredFlags_Default as _) {
        *hovered = None;
    }
    ImGui::Spacing();
    ImGui::Separator();

    let child_sz = ImVec2 { x: 0f32, y: 0f32 };
    if ImGui::BeginChild(c"##gamelist".as_ptr() as _, &child_sz, false, 0) {
        let n = cartridges.len();
        // L / R shoulder buttons page-skip the list. The list is gamepad-nav'd
        // (a Selectable holds the NavId), so we must move the *selection* by a
        // page, not just scroll: scrolling alone leaves NavId behind and the next
        // d-pad press snaps the view back to it. We find the focused row by
        // matching the current NavId, shift it by a page, and re-focus + center
        // the target. Suppressed while an L/R-using overlay (settings tabs) is on
        // top so we don't move the selection in the list hidden behind it.
        let overlay_open = detail_game.is_some() || *show_global_settings;
        let dir = if overlay_open || n == 0 {
            0
        } else if nav_input_pressed(ImGuiNavInput__ImGuiNavInput_FocusPrev) {
            -1
        } else if nav_input_pressed(ImGuiNavInput__ImGuiNavInput_FocusNext) {
            1
        } else {
            0
        };
        let target = if dir != 0 {
            let nav_id = (*ImGui::GetCurrentContext()).NavId;
            let cur = cartridges
                .iter()
                .position(|c| {
                    let name = CString::new(c.file_name.clone()).unwrap();
                    ImGui::GetID(name.as_ptr() as _) == nav_id
                })
                .map_or(0, |c| c as isize);
            let page = ((ImGui::GetWindowHeight() / ImGui::GetTextLineHeightWithSpacing()) as isize).max(1);
            Some((cur + dir * page).clamp(0, n as isize - 1) as usize)
        } else {
            None
        };

        // Clamp selectable to the available width so long filenames clip
        // instead of widening the panel (which makes it horizontally scrollable).
        let sel_width = ImGui::GetContentRegionAvail().x;
        for (i, cartridge) in cartridges.iter().enumerate() {
            let name = CString::new(cartridge.file_name.clone()).unwrap();
            let row_y = ImGui::GetCursorPosY();
            let sel_sz = ImVec2 { x: sel_width, y: 0f32 };
            if ImGui::Selectable(name.as_ptr() as _, false, 0, &sel_sz) {
                *detail_game = Some(i);
                *active_tab = 0;
            }
            if ImGui::IsItemHovered(ImGuiHoveredFlags__ImGuiHoveredFlags_Default as _) {
                *hovered = Some(i);
            }
            if target == Some(i) {
                // Move the gamepad selection here (Selectable just submitted, so
                // its rect feeds NavRectRel) and center the row in view.
                ImGui::SetFocusID(ImGui::GetID(name.as_ptr() as _), (*ImGui::GetCurrentContext()).CurrentWindow);
                ImGui::SetScrollFromPosY(row_y - ImGui::GetScrollY(), 0.5);
            }
        }
    }
    ImGui::EndChild();
    ImGui::End();
}

unsafe fn render_right_panel(cartridges: &[CartridgePreview], icon_tex: u32, hovered: Option<usize>) {
    let top = ImGui::GetFrameHeight();
    if !begin_window(c"##right", 700.0, top, 260.0, 544.0 - top, RIGHT_PANEL_FLAGS) {
        ImGui::End();
        return;
    }
    if let Some(i) = hovered {
        render_game_preview(&cartridges[i], icon_tex);
    } else {
        ImGui::Text(c"Select a game from the list".as_ptr() as _);
    }
    ImGui::End();
}

/// DSi-style tiled menu: a scrolling grid of rounded cartridge-icon tiles. The
/// focused/hovered tile's title, file name and game code show in a bottom bar.
/// Selecting a tile opens the same game detail overlay as the list menu.
/// The imgui id ImageButton assigns to tile `index` — it pushes the texture id
/// then uses `GetID("#image")`, and we wrap each tile in `PushID3(index)`. Needed
/// to match the focused tile against the current NavId for L/R page-skip.
unsafe fn tile_button_id(index: usize, texture: u32) -> u32 {
    ImGui::PushID3(index as _);
    ImGui::PushID2(texture as *const std::ffi::c_void);
    let id = ImGui::GetID(c"#image".as_ptr() as _);
    ImGui::PopID();
    ImGui::PopID();
    id
}

unsafe fn render_tile_grid(
    cartridges: &[CartridgePreview],
    tile_textures: &[u32],
    hovered: &mut Option<usize>,
    show_global_settings: &mut bool,
    detail_game: &mut Option<usize>,
    active_tab: &mut usize,
) {
    let top = ImGui::GetFrameHeight();
    if !begin_window(c"##tiles", 0f32, top, 960f32, 544f32 - top, PANEL_FLAGS) {
        ImGui::End();
        return;
    }

    let style = &*ImGui::GetStyle();

    if full_width_button(c"Global settings") {
        *show_global_settings = true;
    }
    ImGui::Spacing();
    ImGui::Separator();

    // Reserve a two-line footer (title, then file name) below the grid, plus the
    // separator, with no extra bottom gap.
    let footer_height = ImGui::GetTextLineHeightWithSpacing() * 2f32 + style.ItemSpacing.y;

    const TILE: f32 = 96f32;
    const TILE_PADDING: i32 = 8;
    let tile_outer = TILE + (TILE_PADDING * 2) as f32;

    let mut focused = None;
    let grid_sz = ImVec2 { x: 0f32, y: -footer_height };
    if ImGui::BeginChild(c"##tilegrid".as_ptr() as _, &grid_sz, false, 0) {
        let n = tile_textures.len();
        let avail = ImGui::GetContentRegionAvail().x;
        let columns = (((avail + style.ItemSpacing.x) / (tile_outer + style.ItemSpacing.x)) as usize).max(1);

        // L / R shoulder buttons page-skip the selection by a full page of visible
        // rows. Find the focused tile by matching the current NavId, shift it, then
        // re-focus + center the target (same idea as the list menu). Suppressed
        // while an L/R-using overlay is on top.
        let row_height = tile_outer + style.ItemSpacing.y;
        let rows_visible = (ImGui::GetWindowHeight() / row_height) as usize;
        let page = (columns * rows_visible.max(1)).max(1);
        let dir = if detail_game.is_some() || *show_global_settings || n == 0 {
            0
        } else if nav_input_pressed(ImGuiNavInput__ImGuiNavInput_FocusPrev) {
            -1
        } else if nav_input_pressed(ImGuiNavInput__ImGuiNavInput_FocusNext) {
            1
        } else {
            0
        };
        let target = if dir != 0 {
            let nav_id = (*ImGui::GetCurrentContext()).NavId;
            let cur = tile_textures
                .iter()
                .enumerate()
                .position(|(i, texture)| tile_button_id(i, *texture) == nav_id)
                .map_or(0, |c| c as isize);
            Some((cur + dir * page as isize).clamp(0, n as isize - 1) as usize)
        } else {
            None
        };

        // Center the grid block so the leftover width is split evenly instead of
        // pooling on the right edge.
        let used = columns.min(n.max(1)) as f32;
        let grid_width = used * tile_outer + (used - 1f32) * style.ItemSpacing.x;
        let row_start_x = ImGui::GetCursorPosX() + (avail - grid_width).max(0f32) * 0.5;

        let tile_sz = ImVec2 { x: TILE, y: TILE };
        let uv0 = ImVec2 { x: 0f32, y: 0f32 };
        let uv1 = ImVec2 { x: 1f32, y: 1f32 };
        let bg = ImVec4 { x: 0f32, y: 0f32, z: 0f32, w: 0f32 };
        let tint = ImVec4 { x: 1f32, y: 1f32, z: 1f32, w: 1f32 };
        for (i, texture) in tile_textures.iter().enumerate() {
            if i % columns == 0 {
                ImGui::SetCursorPosX(row_start_x);
            } else {
                ImGui::SameLine(0f32, style.ItemSpacing.x);
            }
            let row_y = ImGui::GetCursorPosY();
            ImGui::PushID3(i as _);
            if ImGui::ImageButton(*texture as _, &tile_sz, &uv0, &uv1, TILE_PADDING, &bg, &tint) {
                *detail_game = Some(i);
                *active_tab = 0;
            }
            if ImGui::IsItemHovered(ImGuiHoveredFlags__ImGuiHoveredFlags_Default as _) || ImGui::IsItemFocused() {
                focused = Some(i);
            }
            if target == Some(i) {
                // Move the gamepad selection to this tile (its texture id is on the
                // id stack, matching ImageButton's own id) and center its row.
                ImGui::PushID2(*texture as *const std::ffi::c_void);
                let id = ImGui::GetID(c"#image".as_ptr() as _);
                ImGui::PopID();
                ImGui::SetFocusID(id, (*ImGui::GetCurrentContext()).CurrentWindow);
                ImGui::SetScrollFromPosY(row_y - ImGui::GetScrollY(), 0.5);
            }
            ImGui::PopID();
        }
    }
    ImGui::EndChild();
    if focused.is_some() {
        *hovered = focused;
    }

    ImGui::Separator();
    if let Some(i) = focused {
        let cartridge = &cartridges[i];

        // Only the first line of the title; DS titles are often multi-line.
        let full_title = cartridge.read_title().unwrap_or_else(|_| cartridge.file_name.clone());
        let title = CString::new(full_title.lines().next().unwrap_or("")).unwrap();
        ImGui::Text(title.as_ptr() as _);

        // Game code at the right edge of the title line.
        ImGui::SetWindowFontScale(0.9);
        let game_code = CString::new(format!("{:#010X}", cartridge.get_game_code())).unwrap();
        let code_size = ImGui::CalcTextSize(game_code.as_ptr() as _, ptr::null(), false, 0f32);
        ImGui::SameLine(0f32, 0f32);
        ImGui::SetCursorPosX(ImGui::GetWindowContentRegionMax().x - code_size.x);
        ImGui::TextDisabled(game_code.as_ptr() as _);

        // File name underneath, smaller and disabled.
        ImGui::SetWindowFontScale(0.9);
        let file_name = CString::new(cartridge.file_name.clone()).unwrap();
        ImGui::TextDisabled(file_name.as_ptr() as _);
        ImGui::SetWindowFontScale(1f32);
    }
    ImGui::End();
}

/// 3DS-style menu: cartridge tiles in three evenly-spaced rows that scroll
/// horizontally (column-major), driven by imgui gamepad/keyboard nav. The focused
/// game's title shows above and its file name / code below. L/R page-skip the
/// selection like the other layouts.
unsafe fn render_carousel(
    cartridges: &[CartridgePreview],
    tile_textures: &[u32],
    hovered: &mut Option<usize>,
    show_global_settings: &mut bool,
    detail_game: &mut Option<usize>,
    active_tab: &mut usize,
) {
    const ROWS: usize = 3;
    const TILE: f32 = 100f32;
    const TILE_PADDING: i32 = 8;

    let top = ImGui::GetFrameHeight();
    if !begin_window(c"##carousel", 0f32, top, 960f32, 544f32 - top, PANEL_FLAGS) {
        ImGui::End();
        return;
    }

    let style = &*ImGui::GetStyle();
    if full_width_button(c"Global settings") {
        *show_global_settings = true;
    }
    ImGui::Spacing();
    ImGui::Separator();

    let n = tile_textures.len();

    // Selected game's title (first line), large and centered. Uses last frame's
    // focus (one-frame lag) since the focused tile is only known after the grid.
    let selected = hovered.filter(|&i| i < n).or(if n > 0 { Some(0) } else { None });
    let title_text = selected.map_or(String::new(), |i| {
        let full = cartridges[i].read_title().unwrap_or_else(|_| cartridges[i].file_name.clone());
        full.lines().next().unwrap_or("").to_string()
    });
    let title = CString::new(title_text).unwrap();
    ImGui::SetWindowFontScale(1.4);
    centered_text(&title);
    ImGui::SetWindowFontScale(1.0);
    ImGui::Spacing();

    let footer_height = ImGui::GetTextLineHeightWithSpacing() + style.ItemSpacing.y;

    let tile_outer = TILE + (TILE_PADDING * 2) as f32;
    let cell_w = tile_outer + style.ItemSpacing.x;
    let cell_h = tile_outer + style.ItemSpacing.y;

    let mut focused = None;
    let grid_sz = ImVec2 { x: 0f32, y: -footer_height };
    if ImGui::BeginChild(c"##carouselgrid".as_ptr() as _, &grid_sz, false, ImGuiWindowFlags__ImGuiWindowFlags_HorizontalScrollbar as _) {
        let child_w = ImGui::GetContentRegionAvail().x;
        let base_x = ImGui::GetCursorPosX();
        // Center the three rows vertically in the child.
        let base_y = ImGui::GetCursorPosY() + ((ImGui::GetContentRegionAvail().y - ROWS as f32 * cell_h) * 0.5).max(0f32);

        // L / R shoulder buttons page-skip the selection by a screen of columns.
        let visible_cols = (child_w / cell_w) as usize;
        let page = (ROWS * visible_cols.max(1)).max(1);
        let dir = if detail_game.is_some() || *show_global_settings || n == 0 {
            0
        } else if nav_input_pressed(ImGuiNavInput__ImGuiNavInput_FocusPrev) {
            -1
        } else if nav_input_pressed(ImGuiNavInput__ImGuiNavInput_FocusNext) {
            1
        } else {
            0
        };
        let target = if dir != 0 {
            let nav_id = (*ImGui::GetCurrentContext()).NavId;
            let cur = tile_textures
                .iter()
                .enumerate()
                .position(|(i, texture)| tile_button_id(i, *texture) == nav_id)
                .map_or(0, |c| c as isize);
            Some((cur + dir * page as isize).clamp(0, n as isize - 1) as usize)
        } else {
            None
        };

        let tile_sz = ImVec2 { x: TILE, y: TILE };
        let uv0 = ImVec2 { x: 0f32, y: 0f32 };
        let uv1 = ImVec2 { x: 1f32, y: 1f32 };
        let bg = ImVec4 { x: 0f32, y: 0f32, z: 0f32, w: 0f32 };
        let tint = ImVec4 { x: 1f32, y: 1f32, z: 1f32, w: 1f32 };
        for (i, texture) in tile_textures.iter().enumerate() {
            let col = i / ROWS;
            let row = i % ROWS;
            let pos = ImVec2 {
                x: base_x + col as f32 * cell_w,
                y: base_y + row as f32 * cell_h,
            };
            ImGui::SetCursorPos(&pos);
            ImGui::PushID3(i as _);
            if ImGui::ImageButton(*texture as _, &tile_sz, &uv0, &uv1, TILE_PADDING, &bg, &tint) {
                *detail_game = Some(i);
                *active_tab = 0;
            }
            if ImGui::IsItemHovered(ImGuiHoveredFlags__ImGuiHoveredFlags_Default as _) || ImGui::IsItemFocused() {
                focused = Some(i);
            }
            if target == Some(i) {
                ImGui::PushID2(*texture as *const std::ffi::c_void);
                let id = ImGui::GetID(c"#image".as_ptr() as _);
                ImGui::PopID();
                ImGui::SetFocusID(id, (*ImGui::GetCurrentContext()).CurrentWindow);
                // Bring the target column into view, centered (nav alone doesn't scroll).
                ImGui::SetScrollX((col as f32 * cell_w - (child_w - cell_w) * 0.5).max(0f32));
            }
            ImGui::PopID();
        }
    }
    ImGui::EndChild();
    if focused.is_some() {
        *hovered = focused;
    }

    // Footer: file name (smaller) and game code at the right edge.
    ImGui::Separator();
    if let Some(i) = focused.or(selected) {
        let cartridge = &cartridges[i];
        ImGui::SetWindowFontScale(0.9);
        let file_name = CString::new(cartridge.file_name.clone()).unwrap();
        ImGui::TextDisabled(file_name.as_ptr() as _);
        let game_code = CString::new(format!("{:#010X}", cartridge.get_game_code())).unwrap();
        let code_size = ImGui::CalcTextSize(game_code.as_ptr() as _, ptr::null(), false, 0f32);
        ImGui::SameLine(0f32, 0f32);
        ImGui::SetCursorPosX(ImGui::GetWindowContentRegionMax().x - code_size.x);
        ImGui::TextDisabled(game_code.as_ptr() as _);
        ImGui::SetWindowFontScale(1f32);
    }
    ImGui::End();
}

/// Wrapped bullet line for the recommendations dialog. BulletText itself ignores
/// the wrap pos (it renders one long line, blowing up the auto-resized window), so
/// draw the bullet, then wrapped TextUnformatted (also avoids printf parsing the
/// text, so a `%` in a value is safe).
unsafe fn dialog_bullet(text: &std::ffi::CStr, wrap: f32) {
    ImGui::Bullet();
    ImGui::SameLine(0f32, (*ImGui::GetStyle()).ItemInnerSpacing.x);
    ImGui::PushTextWrapPos(wrap);
    ImGui::TextUnformatted(text.as_ptr() as _, ptr::null());
    ImGui::PopTextWrapPos();
}

/// Modal with a game's recommended settings, or — when none are known — a default
/// notice with general hints. Opened via `OpenPopup(c"game_info_dialog")`.
unsafe fn render_game_info_dialog(info: Option<&crate::game_info::SettingRecommendation>) {
    center_next_window();
    if ImGui::BeginPopupModal(
        c"game_info_dialog".as_ptr(),
        ptr::null_mut(),
        (ImGuiWindowFlags__ImGuiWindowFlags_NoTitleBar
            | ImGuiWindowFlags__ImGuiWindowFlags_NoResize
            | ImGuiWindowFlags__ImGuiWindowFlags_NoMove
            | ImGuiWindowFlags__ImGuiWindowFlags_NoCollapse
            | ImGuiWindowFlags__ImGuiWindowFlags_AlwaysAutoResize) as _,
    ) {
        // Bound the popup width by wrapping the text; the setting lines are short.
        let wrap = ImGui::GetCursorPosX() + 420f32;

        match info {
            Some(info) => {
                dialog_title(c"Recommended settings");

                // Plain-language warning: these recommendations may be wrong.
                let notice = c"These recommendations can be wrong: if the game runs badly or crashes, change the settings and try again.";
                ImGui::PushTextWrapPos(wrap);
                ImGui::Text(notice.as_ptr() as _);
                ImGui::PopTextWrapPos();
                ImGui::Spacing();
                ImGui::Separator();
                ImGui::Spacing();

                for (id, value) in &info.settings {
                    let definition = id.definition();
                    let group: &str = definition.group.into();
                    let value_text = match value {
                        SettingValue::Bool(value) => (if *value { "on" } else { "off" }).to_string(),
                        SettingValue::Int(index) => definition.value.as_list().and_then(|(_, values)| values.get(*index)).cloned().unwrap_or_else(|| index.to_string()),
                        SettingValue::List(inner) => inner.values.get(inner.selection).cloned().unwrap_or_default(),
                        SettingValue::Slider(inner) => inner.value.to_string(),
                    };
                    let line = CString::new(format!("{} - {}: {}", group, definition.title, value_text)).unwrap();
                    dialog_bullet(&line, wrap);
                }

                if !info.info.is_empty() {
                    ImGui::Spacing();
                    let info_text = CString::new(info.info).unwrap();
                    ImGui::PushTextWrapPos(wrap);
                    ImGui::TextDisabled(info_text.as_ptr() as _);
                    ImGui::PopTextWrapPos();
                }
            }
            None => {
                dialog_title(c"No recommendations");

                let intro = c"There are no recommendations for this game yet. Please try it on your own. These general hints may help:";
                ImGui::PushTextWrapPos(wrap);
                ImGui::Text(intro.as_ptr() as _);
                ImGui::PopTextWrapPos();
                ImGui::Spacing();

                dialog_bullet(c"Start with Arm7 Emulation set to Hle - it usually works and runs the fastest. If the game crashes or freezes at some point, switch to SoundHle instead, which is still faster than AccurateLle.", wrap);
                dialog_bullet(c"Same idea for the HLE OS irq handler: turn it off if the game crashes or freezes.", wrap);
                dialog_bullet(c"If the game has heavy graphical glitches or uses 3D on both screen, turn Geometry 3D frameskip off.", wrap);
            }
        }
        ImGui::Spacing();

        if menu_button(c"Close", 200f32) {
            ImGui::CloseCurrentPopup();
        }
        ImGui::EndPopup();
    }
}

unsafe fn render_game_detail_overlay(
    cartridges: &[CartridgePreview],
    settings_configs: &mut [SettingsConfig],
    screen_layouts: &ScreenLayouts,
    global_settings: &GlobalSettings,
    icon_tex: u32,
    detail_game: &mut Option<usize>,
    active_tab: &mut usize,
    overlay_focused: &mut bool,
    launched: &mut bool,
) {
    let Some(i) = *detail_game else {
        *overlay_focused = true;
        return;
    };
    if !begin_fullscreen_overlay(c"##gamedetail") {
        ImGui::End();
        return;
    }
    let cartridge = &cartridges[i];
    let game_info = get_game_info(cartridge.get_game_code());

    icon_image(icon_tex);
    ImGui::SameLine(0.0, 12.0);
    ImGui::BeginGroup();
    ImGui::SetWindowFontScale(1.3);
    let title = CString::new(cartridge.read_title().unwrap_or("Couldn't read game title".to_string())).unwrap();
    ImGui::Text(title.as_ptr() as _);

    ImGui::SetWindowFontScale(0.9);
    let game_code = CString::new(format!("{:#010X}", cartridge.get_game_code())).unwrap();
    let code_size = ImGui::CalcTextSize(game_code.as_ptr() as _, ptr::null(), false, 0f32);
    ImGui::SameLine(0f32, 0f32);
    ImGui::SetCursorPosX(ImGui::GetWindowContentRegionMax().x - code_size.x);
    ImGui::TextDisabled(game_code.as_ptr() as _);
    ImGui::SetWindowFontScale(1.0);
    ImGui::Spacing();
    // Launch fills the row except for the recommendations button beside it, which
    // is sized to its own label and always shown (it falls back to general hints).
    let spacing = (*ImGui::GetStyle()).ItemSpacing.x;
    let info_label = c"Recommended";
    let info_w = ImGui::CalcTextSize(info_label.as_ptr(), ptr::null(), false, 0f32).x + (*ImGui::GetStyle()).FramePadding.x * 2f32;
    let launch_sz = ImVec2 {
        x: ImGui::GetContentRegionAvail().x - info_w - spacing,
        y: 0f32,
    };
    if ImGui::Button(c"Launch game".as_ptr(), &launch_sz) {
        *launched = true;
    }
    ImGui::SameLine(0f32, spacing);
    let info_sz = ImVec2 { x: info_w, y: 0f32 };
    // Red button when this game has specific recommendations, so it stands out
    // from the generic hints shown for unknown games
    if game_info.is_some() {
        ImGui::PushStyleColor(ImGuiCol__ImGuiCol_Button as _, 0xFF2020B0);
        ImGui::PushStyleColor(ImGuiCol__ImGuiCol_ButtonHovered as _, 0xFF3030D0);
        ImGui::PushStyleColor(ImGuiCol__ImGuiCol_ButtonActive as _, 0xFF181890);
    }
    if ImGui::Button(info_label.as_ptr(), &info_sz) {
        ImGui::OpenPopup(c"game_info_dialog".as_ptr());
    }
    if game_info.is_some() {
        ImGui::PopStyleColor(3);
    }
    ImGui::PushTextWrapPos(0f32);
    ImGui::TextDisabled(c"First launch will take a while. Don't exit or power off your Vita.".as_ptr());
    ImGui::PopTextWrapPos();
    ImGui::EndGroup();

    render_game_info_dialog(game_info);

    ImGui::Spacing();
    settings_configs[i].settings.populate_screen_layouts(screen_layouts);
    settings_configs[i].settings.populate_controls(&global_settings.default_control, &global_settings.custom_controls);
    render_settings_tabs(&mut settings_configs[i], active_tab, false, ImGui::GetFrameHeightWithSpacing());

    // Pin the Save button to the bottom of the overlay so it never floats up
    // with the scroll list above it.
    ImGui::SetCursorPosY(ImGui::GetWindowHeight() - (*ImGui::GetStyle()).WindowPadding.y - ImGui::GetFrameHeight());

    let settings_config = &mut settings_configs[i];
    let dirty = settings_config.dirty;
    if !dirty {
        ImGui::PushItemFlag(ImGuiItemFlags__ImGuiItemFlags_Disabled as _, true);
        ImGui::PushStyleVar(ImGuiStyleVar__ImGuiStyleVar_Alpha as _, (*ImGui::GetStyle()).Alpha * 0.5f32);
    }
    if full_width_button(c"Save settings") {
        settings_config.flush();
    }
    if !dirty {
        ImGui::PopItemFlag();
        ImGui::PopStyleVar(1);
    }

    // Back/Esc: only leave the menu when at the top level; otherwise imgui has
    // already stepped out of the settings child or a combo this frame.
    if back_closes_overlay(*overlay_focused) {
        *detail_game = None;
    }
    *overlay_focused = ImGui::IsWindowFocused(0);
    ImGui::End();
}

unsafe fn render_game_preview(cartridge: &CartridgePreview, icon_tex: u32) {
    icon_image(icon_tex);
    match cartridge.read_title() {
        Ok(title) => {
            let title = CString::new(title).unwrap();
            ImGui::Text(title.as_ptr() as _);
        }
        Err(_) => ImGui::Text(c"Couldn't read game title".as_ptr() as _),
    }
}
struct SavestateUiEntry {
    path: PathBuf,
    label: CString,
    detail: CString,
    // Row-spanning selectable id; the visible content is drawlist-drawn on top
    sel_id: CString,
    texture: u32,
    arm7_matches: bool,
}

fn rgb_to_rgba(rgb: &[u8], pixels: usize) -> Vec<u8> {
    let mut rgba = vec![0u8; pixels * 4];
    for i in 0..pixels {
        rgba[i * 4..i * 4 + 3].copy_from_slice(&rgb[i * 3..i * 3 + 3]);
        rgba[i * 4 + 3] = 0xFF;
    }
    rgba
}

fn decode_screenshot_jpeg(bytes: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let mut decoder = jpeg_decoder::Decoder::new(std::io::Cursor::new(bytes));
    let data = decoder.decode().ok()?;
    let info = decoder.info()?;
    if info.pixel_format != jpeg_decoder::PixelFormat::RGB24 {
        return None;
    }
    let pixels = info.width as usize * info.height as usize;
    Some((info.width as u32, info.height as u32, rgb_to_rgba(&data, pixels)))
}

fn decode_screenshot_png(bytes: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0u8; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    if info.bit_depth != png::BitDepth::Eight {
        return None;
    }
    let data = match info.color_type {
        png::ColorType::Rgba => {
            buf.truncate(info.buffer_size());
            buf
        }
        png::ColorType::Rgb => rgb_to_rgba(&buf, (info.width * info.height) as usize),
        _ => return None,
    };
    Some((info.width, info.height, data))
}

/// Decode an embedded screenshot (jpeg, or png from older savestates) into a GL
/// texture; 0 on any failure (entry then renders without a thumbnail).
unsafe fn create_screenshot_texture(bytes: &[u8]) -> u32 {
    let decoded = match bytes {
        [0xFF, 0xD8, ..] => decode_screenshot_jpeg(bytes),
        [0x89, b'P', b'N', b'G', ..] => decode_screenshot_png(bytes),
        _ => None,
    };
    let Some((width, height, data)) = decoded else { return 0 };
    let mut tex = 0;
    gl::GenTextures(1, &mut tex);
    gl::BindTexture(gl::TEXTURE_2D, tex);
    gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::LINEAR as _);
    gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::LINEAR as _);
    gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as _);
    gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as _);
    gl::TexImage2D(gl::TEXTURE_2D, 0, gl::RGBA as _, width as _, height as _, 0, gl::RGBA, gl::UNSIGNED_BYTE, data.as_ptr() as _);
    tex
}

pub fn savestates_dir(rom_path: &Path) -> PathBuf {
    rom_path.parent().unwrap_or(Path::new(".")).join("savestates")
}

/// Scan `<rom_dir>/savestates/<stem>-<N>.sav`, newest number first. Entries whose
/// arm7 emulation mode differs from the running session can't be loaded.
unsafe fn load_savestate_entries(rom_path: &Path, current_arm7: u8) -> Vec<SavestateUiEntry> {
    let mut entries = Vec::new();
    let stem = rom_path.file_stem().unwrap_or_default().to_string_lossy().into_owned();
    let prefix = format!("{stem}-");
    let Ok(read_dir) = fs::read_dir(savestates_dir(rom_path)) else { return entries };

    let mut found = Vec::new();
    for dir_entry in read_dir.flatten() {
        let name = dir_entry.file_name().to_string_lossy().into_owned();
        let Some(num) = name.strip_prefix(&prefix).and_then(|rest| rest.strip_suffix(".sav")).and_then(|num| num.parse::<u32>().ok()) else {
            continue;
        };
        found.push((num, dir_entry.path()));
    }
    found.sort_by(|a, b| b.0.cmp(&a.0));

    for (num, path) in found {
        let Some(meta) = crate::savestate::peek_meta(&path) else { continue };
        let arm7_matches = meta.arm7_emu == current_arm7;
        let metadata = fs::metadata(&path).ok();
        let modified = metadata
            .as_ref()
            .and_then(|m| m.modified().ok())
            .map(|time| chrono::DateTime::<chrono::Local>::from(time).format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_default();
        let size = metadata.map(|m| format!(" - {:.1} MB", m.len() as f64 / (1024.0 * 1024.0))).unwrap_or_default();
        let detail = if arm7_matches {
            format!("{modified}{size}")
        } else {
            let state_mode: &'static str = crate::settings::Arm7Emu::from(meta.arm7_emu).into();
            let session_mode: &'static str = crate::settings::Arm7Emu::from(current_arm7).into();
            format!("{modified}{size} - ARM7 emulation: {state_mode} (session: {session_mode})")
        };
        entries.push(SavestateUiEntry {
            texture: create_screenshot_texture(&meta.screenshot),
            path,
            label: CString::new(format!("Savestate {num}")).unwrap(),
            detail: CString::new(detail).unwrap(),
            sel_id: CString::new(format!("##savestate{num}")).unwrap(),
            arm7_matches,
        });
    }
    entries
}

unsafe fn free_savestate_entries(entries: &mut Vec<SavestateUiEntry>) {
    for entry in entries.iter() {
        if entry.texture != 0 {
            gl::DeleteTextures(1, &entry.texture);
        }
    }
    entries.clear();
}

enum SavestateUiAction {
    Create,
    Load(PathBuf),
    Delete(PathBuf),
}

/// Full-screen savestate list: thumbnail and name/date per entry, create button on
/// top. Clicking an entry opens a modal offering Load (arm7-gated) or Delete.
unsafe fn render_savestate_overlay(entries: &[SavestateUiEntry], selected: &mut Option<usize>) -> Option<SavestateUiAction> {
    let mut action = None;

    if full_width_button(c"Create new savestate") {
        action = Some(SavestateUiAction::Create);
    }
    ImGui::Spacing();
    ImGui::Separator();
    ImGui::Spacing();

    if entries.is_empty() {
        ImGui::Spacing();
        centered_text(c"No savestates yet");
    }

    let child_sz = ImVec2 { x: 0.0, y: 0.0 };
    if ImGui::BeginChild(c"##savestate_scroll".as_ptr() as _, &child_sz, false, 0) {
        // Thumbnails keep the presenter aspect (960x544)
        const THUMB_W: f32 = 240.0;
        const THUMB_H: f32 = 136.0;
        for (i, entry) in entries.iter().enumerate() {
            // One row-spanning Selectable so the entry is reachable by gamepad/
            // keyboard nav (a plain widget group never receives nav focus); the
            // thumbnail and texts are drawlist-drawn over it so no other item
            // competes for hover/clicks.
            let origin = ImGui::GetCursorScreenPos();
            let sel_sz = ImVec2 {
                x: ImGui::GetContentRegionAvail().x,
                y: THUMB_H,
            };
            if ImGui::Selectable(entry.sel_id.as_ptr(), false, 0, &sel_sz) {
                *selected = Some(i);
            }

            let dl = ImGui::GetWindowDrawList();
            if entry.texture != 0 {
                let thumb_min = origin;
                let thumb_max = ImVec2 {
                    x: origin.x + THUMB_W,
                    y: origin.y + THUMB_H,
                };
                let uv0 = ImVec2 { x: 0.0, y: 0.0 };
                let uv1 = ImVec2 { x: 1.0, y: 1.0 };
                ImDrawList_AddImage(dl, entry.texture as _, &thumb_min, &thumb_max, &uv0, &uv1, 0xFFFFFFFF);
                ImDrawList_AddRect(dl, &thumb_min, &thumb_max, 0xFF4D4D4D, 0.0, 0, 1.0);
            }
            let line_height = ImGui::GetTextLineHeightWithSpacing();
            let label_pos = ImVec2 {
                x: origin.x + THUMB_W + 14.0,
                y: origin.y + 4.0,
            };
            let detail_pos = ImVec2 {
                x: label_pos.x,
                y: label_pos.y + line_height * 1.3,
            };
            ImDrawList_AddText(dl, &label_pos, 0xFFFFFFFF, entry.label.as_ptr(), ptr::null());
            ImDrawList_AddText(dl, &detail_pos, 0xFFCCCCCC, entry.detail.as_ptr(), ptr::null());

            ImGui::Spacing();
            ImGui::Separator();
            ImGui::Spacing();
        }
    }
    ImGui::EndChild();

    // Load-or-delete dialog for the clicked entry
    if let Some(i) = *selected {
        let entry = &entries[i];
        ImGui::OpenPopup(c"SavestateActionPopup".as_ptr());
        center_next_window();
        const MODAL_FLAGS: u32 = (ImGuiWindowFlags__ImGuiWindowFlags_NoTitleBar
            | ImGuiWindowFlags__ImGuiWindowFlags_NoResize
            | ImGuiWindowFlags__ImGuiWindowFlags_NoMove
            | ImGuiWindowFlags__ImGuiWindowFlags_NoCollapse
            | ImGuiWindowFlags__ImGuiWindowFlags_AlwaysAutoResize) as u32;
        if ImGui::BeginPopupModal(c"SavestateActionPopup".as_ptr(), ptr::null_mut(), MODAL_FLAGS as _) {
            dialog_title(&entry.label);
            centered_text(&entry.detail);
            ImGui::Spacing();
            const BUTTON_WIDTH: f32 = 260.0;
            if !entry.arm7_matches {
                ImGui::PushItemFlag(ImGuiItemFlags__ImGuiItemFlags_Disabled as _, true);
                ImGui::PushStyleVar(ImGuiStyleVar__ImGuiStyleVar_Alpha as _, (*ImGui::GetStyle()).Alpha * 0.5f32);
            }
            if menu_button(c"Load", BUTTON_WIDTH) {
                action = Some(SavestateUiAction::Load(entry.path.clone()));
                *selected = None;
                ImGui::CloseCurrentPopup();
            }
            if !entry.arm7_matches {
                ImGui::PopItemFlag();
                ImGui::PopStyleVar(1);
            }
            if menu_button(c"Delete", BUTTON_WIDTH) {
                action = Some(SavestateUiAction::Delete(entry.path.clone()));
                *selected = None;
                ImGui::CloseCurrentPopup();
            }
            if menu_button(c"Cancel", BUTTON_WIDTH) || cancel_pressed() {
                *selected = None;
                ImGui::CloseCurrentPopup();
            }
            ImGui::EndPopup();
        }
    }

    action
}

pub use crate::presenter::UiPauseMenuReturn;

pub fn show_pause_menu(ui_backend: &mut impl UiBackend, gpu_renderer: &GpuRenderer, settings: &mut Settings, settings_file_path: &std::path::Path, rom_path: &Path) -> UiPauseMenuReturn {
    let mut pressed_settings = false;
    let mut pressed_savestates = false;
    let mut savestate_entries: Vec<SavestateUiEntry> = Vec::new();
    let mut savestate_selected: Option<usize> = None;
    let mut pressed_cheats = false;
    // Snapshot of the shared cheat list for rendering; the `##i` suffix keeps imgui
    // ids unique when cheat names repeat
    let mut cheat_entries: Vec<(CString, bool, bool)> = Vec::new();
    let mut pressed_quit = false;
    let mut pressed_exit = false;
    let mut return_value = None;
    let mut settings_config = SettingsConfig::from(settings.clone());
    // Path of the per-game ini, so runtime changes can be persisted from here.
    // Empty when launched without a settings file (e.g. direct CLI launch).
    settings_config.settings_file_path = settings_file_path.to_path_buf();
    let savable = !settings_file_path.as_os_str().is_empty();
    let mut active_tab: usize = 0;
    let mut overlay_focused = true;
    loop {
        unsafe {
            gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
            gl::Viewport(0, 0, PRESENTER_SCREEN_WIDTH as _, PRESENTER_SCREEN_HEIGHT as _);
            gl::ClearColor(0f32, 0f32, 0f32, 1f32);
            gl::Clear(gl::COLOR_BUFFER_BIT);
            gpu_renderer.blit_main_framebuffer();

            if !ui_backend.new_frame() {
                return UiPauseMenuReturn::QuitApp;
            }

            const MODAL_FLAGS: u32 = (ImGuiWindowFlags__ImGuiWindowFlags_NoTitleBar
                | ImGuiWindowFlags__ImGuiWindowFlags_NoResize
                | ImGuiWindowFlags__ImGuiWindowFlags_NoMove
                | ImGuiWindowFlags__ImGuiWindowFlags_NoCollapse
                | ImGuiWindowFlags__ImGuiWindowFlags_AlwaysAutoResize) as u32;

            center_next_window();
            if ImGui::BeginPopupModal(c"PausePopup".as_ptr(), ptr::null_mut(), MODAL_FLAGS as _) {
                dialog_title(c"Paused");
                const BUTTON_WIDTH: f32 = 260.0;
                if menu_button(c"Resume", BUTTON_WIDTH) {
                    return_value = Some(UiPauseMenuReturn::Resume);
                    ImGui::CloseCurrentPopup();
                }
                if menu_button(c"Settings", BUTTON_WIDTH) {
                    pressed_settings = true;
                    active_tab = 0;
                    overlay_focused = true;
                    ImGui::CloseCurrentPopup();
                }
                if menu_button(c"Savestates", BUTTON_WIDTH) {
                    pressed_savestates = true;
                    overlay_focused = true;
                    savestate_selected = None;
                    savestate_entries = load_savestate_entries(rom_path, settings.arm7_emu() as u8);
                    ImGui::CloseCurrentPopup();
                }
                if menu_button(c"Cheats", BUTTON_WIDTH) {
                    pressed_cheats = true;
                    overlay_focused = true;
                    cheat_entries = crate::cheats::names_and_states()
                        .into_iter()
                        .enumerate()
                        .map(|(i, (name, has_code, enabled))| (CString::new(if has_code { format!("{name}##cheat{i}") } else { name }).unwrap(), has_code, enabled))
                        .collect();
                    ImGui::CloseCurrentPopup();
                }
                if menu_button(c"Blow into mic", BUTTON_WIDTH) {
                    return_value = Some(UiPauseMenuReturn::BlowMic);
                    ImGui::CloseCurrentPopup();
                }
                if menu_button(c"Quit game", BUTTON_WIDTH) {
                    pressed_quit = true;
                    ImGui::CloseCurrentPopup();
                }
                if menu_button(c"Exit emulator", BUTTON_WIDTH) {
                    pressed_exit = true;
                    ImGui::CloseCurrentPopup();
                }
                ImGui::EndPopup();
            }

            center_next_window();
            if ImGui::BeginPopupModal(c"QuitPopup".as_ptr(), ptr::null_mut(), MODAL_FLAGS as _) {
                dialog_title(c"Exit game?");
                centered_text(c"Unsaved progress will be lost.");
                ImGui::Spacing();
                ImGui::Spacing();

                const BUTTON_WIDTH: f32 = 120.0;
                let spacing = (*ImGui::GetStyle()).ItemSpacing.x;
                let total = BUTTON_WIDTH * 2.0 + spacing;
                let avail = ImGui::GetContentRegionAvail().x;
                if avail > total {
                    ImGui::SetCursorPosX(ImGui::GetCursorPosX() + (avail - total) * 0.5);
                }
                let bsz = ImVec2 { x: BUTTON_WIDTH, y: 44.0 };
                if ImGui::Button(c"No".as_ptr(), &bsz) {
                    pressed_quit = false;
                    pressed_exit = false;
                    ImGui::CloseCurrentPopup();
                }
                ImGui::SameLine(0.0, spacing);
                if ImGui::Button(c"Yes".as_ptr(), &bsz) {
                    return_value = if pressed_exit { Some(UiPauseMenuReturn::QuitApp) } else { Some(UiPauseMenuReturn::Quit) };
                    ImGui::CloseCurrentPopup();
                }
                ImGui::EndPopup();
            }

            if return_value.is_none() {
                if pressed_settings {
                    if begin_fullscreen_overlay(c"##details") {
                        let reserve = if savable { ImGui::GetFrameHeightWithSpacing() } else { 0f32 };
                        render_settings_tabs(&mut settings_config, &mut active_tab, true, reserve);

                        // Persist runtime changes to the game's ini. Pinned to the
                        // bottom and disabled until something changed.
                        if savable {
                            ImGui::SetCursorPosY(ImGui::GetWindowHeight() - (*ImGui::GetStyle()).WindowPadding.y - ImGui::GetFrameHeight());
                            let dirty = settings_config.dirty;
                            if !dirty {
                                ImGui::PushItemFlag(ImGuiItemFlags__ImGuiItemFlags_Disabled as _, true);
                                ImGui::PushStyleVar(ImGuiStyleVar__ImGuiStyleVar_Alpha as _, (*ImGui::GetStyle()).Alpha * 0.5f32);
                            }
                            if full_width_button(c"Save settings") {
                                settings_config.flush();
                            }
                            if !dirty {
                                ImGui::PopItemFlag();
                                ImGui::PopStyleVar(1);
                            }
                        }
                        // Back/Esc steps out of the settings child first; only
                        // closes the settings menu when at the tab level.
                        if back_closes_overlay(overlay_focused) {
                            pressed_settings = false;
                        }
                        overlay_focused = ImGui::IsWindowFocused(0);
                    }
                    ImGui::End();
                } else if pressed_savestates {
                    if begin_fullscreen_overlay(c"##savestates") {
                        match render_savestate_overlay(&savestate_entries, &mut savestate_selected) {
                            Some(SavestateUiAction::Create) => {
                                // Screenshot of the frozen frame behind the menu; the state
                                // itself is written by the cpu thread at the next vblank.
                                // op_begin arms the progress dialog main.rs shows meanwhile.
                                let screenshot = gpu_renderer.capture_main_framebuffer_jpeg();
                                crate::savestate::op_begin();
                                crate::savestate::request_save_with_screenshot(screenshot);
                                return_value = Some(UiPauseMenuReturn::Resume);
                            }
                            Some(SavestateUiAction::Load(path)) => match fs::read(&path) {
                                Ok(data) => {
                                    crate::savestate::op_begin();
                                    crate::savestate::request_load(data);
                                    return_value = Some(UiPauseMenuReturn::Resume);
                                }
                                Err(err) => {
                                    eprintln!("Failed to read savestate {path:?}: {err}");
                                }
                            },
                            Some(SavestateUiAction::Delete(path)) => {
                                if let Err(err) = fs::remove_file(&path) {
                                    eprintln!("Failed to delete savestate {path:?}: {err}");
                                }
                                free_savestate_entries(&mut savestate_entries);
                                savestate_entries = load_savestate_entries(rom_path, settings.arm7_emu() as u8);
                            }
                            None => {}
                        }
                        // Close the overlay only when no entry dialog is open
                        if savestate_selected.is_none() && back_closes_overlay(overlay_focused) {
                            pressed_savestates = false;
                            free_savestate_entries(&mut savestate_entries);
                        }
                        overlay_focused = ImGui::IsWindowFocused(0);
                    }
                    ImGui::End();
                } else if pressed_cheats {
                    if begin_fullscreen_overlay(c"##cheats") {
                        if cheat_entries.is_empty() {
                            ImGui::Spacing();
                            centered_text(c"No cheats found");
                            ImGui::Spacing();
                            let hint = CString::new(format!("Place cheats in {}", path::absolute(crate::cheats::cheats_path(rom_path)).unwrap().display())).unwrap_or_default();
                            centered_text(&hint);
                            centered_text(c"Download them from https://github.com/libretro/libretro-database");
                        } else {
                            let child_sz = ImVec2 { x: 0.0, y: 0.0 };
                            if ImGui::BeginChild(c"##cheat_scroll".as_ptr() as _, &child_sz, false, 0) {
                                for (i, (label, has_code, enabled)) in cheat_entries.iter_mut().enumerate() {
                                    // Toggles hit the shared list immediately and persist
                                    // to the .cht file; the cpu thread is parked while the
                                    // menu is open, so cheats take effect on resume
                                    if *has_code {
                                        if ImGui::Checkbox(label.as_ptr(), enabled) {
                                            crate::cheats::set_enabled(i, *enabled);
                                            if let Err(err) = crate::cheats::save(rom_path) {
                                                eprintln!("Failed to save cheats: {err}");
                                            }
                                        }
                                    } else {
                                        ImGui::Text(label.as_ptr());
                                    }
                                }
                            }
                            ImGui::EndChild();
                        }
                        if back_closes_overlay(overlay_focused) {
                            pressed_cheats = false;
                        }
                        overlay_focused = ImGui::IsWindowFocused(0);
                    }
                    ImGui::End();
                } else if pressed_quit || pressed_exit {
                    ImGui::OpenPopup(c"QuitPopup".as_ptr());
                } else {
                    ImGui::OpenPopup(c"PausePopup".as_ptr());
                }
            }

            ImGui::Render();
            ui_backend.render_draw_data(ImGui::GetDrawData());
            ui_backend.swap_window();

            if let Some(ret) = return_value {
                free_savestate_entries(&mut savestate_entries);
                // Apply unconditionally: a Save clears `dirty`, so gating on it
                // would drop the runtime changes the user just saved. Copying back
                // unchanged settings is a harmless no-op.
                *settings = settings_config.settings;
                return ret;
            }
        }
    }
}
pub fn show_progress(ui_backend: &mut impl UiBackend, current_name: impl AsRef<str>, progress: usize, total: usize) {
    unsafe {
        gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
        gl::Viewport(0, 0, PRESENTER_SCREEN_WIDTH as _, PRESENTER_SCREEN_HEIGHT as _);
        gl::ClearColor(0f32, 0f32, 0f32, 1f32);
        gl::Clear(gl::COLOR_BUFFER_BIT);

        ui_backend.new_frame();

        center_next_window();
        if ImGui::BeginPopupModal(
            c"ProgressPopup".as_ptr(),
            ptr::null_mut(),
            (ImGuiWindowFlags__ImGuiWindowFlags_NoTitleBar
                | ImGuiWindowFlags__ImGuiWindowFlags_NoResize
                | ImGuiWindowFlags__ImGuiWindowFlags_NoMove
                | ImGuiWindowFlags__ImGuiWindowFlags_NoCollapse
                | ImGuiWindowFlags__ImGuiWindowFlags_AlwaysAutoResize) as _,
        ) {
            dialog_title(c"Loading");
            centered_text(c"If you are stuck here, make sure you have");
            centered_text(c"kubridge version 0.3.1 installed!");
            centered_text(c"If you experience a crash here, remove shader cache in ux0:/data/shader_cache");
            ImGui::Spacing();
            let text = CString::from_str(current_name.as_ref()).unwrap();
            centered_text(&text);
            ImGui::Spacing();
            const BAR_WIDTH: f32 = 440.0;
            let avail = ImGui::GetContentRegionAvail().x;
            if avail > BAR_WIDTH {
                ImGui::SetCursorPosX(ImGui::GetCursorPosX() + (avail - BAR_WIDTH) * 0.5);
            }
            let sz = ImVec2 { x: BAR_WIDTH, y: 28.0 };
            ImGui::ProgressBar(progress as f32 / total as f32, &sz, ptr::null());
            ImGui::EndPopup();
        }
        ImGui::OpenPopup(c"ProgressPopup".as_ptr());

        ImGui::Render();
        ui_backend.render_draw_data(ImGui::GetDrawData());
        ui_backend.swap_window();
    }
}

/// One frame of the savestate progress dialog, drawn over the frozen game frame
/// while main.rs waits for the cpu thread to finish a menu-requested save/load.
pub fn show_savestate_progress(ui_backend: &mut impl UiBackend, gpu_renderer: &GpuRenderer, text: impl AsRef<str>, progress: usize) {
    unsafe {
        gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
        gl::Viewport(0, 0, PRESENTER_SCREEN_WIDTH as _, PRESENTER_SCREEN_HEIGHT as _);
        gl::ClearColor(0f32, 0f32, 0f32, 1f32);
        gl::Clear(gl::COLOR_BUFFER_BIT);
        gpu_renderer.blit_main_framebuffer();

        ui_backend.new_frame();

        center_next_window();
        if ImGui::BeginPopupModal(
            c"SavestateProgressPopup".as_ptr(),
            ptr::null_mut(),
            (ImGuiWindowFlags__ImGuiWindowFlags_NoTitleBar
                | ImGuiWindowFlags__ImGuiWindowFlags_NoResize
                | ImGuiWindowFlags__ImGuiWindowFlags_NoMove
                | ImGuiWindowFlags__ImGuiWindowFlags_NoCollapse
                | ImGuiWindowFlags__ImGuiWindowFlags_AlwaysAutoResize) as _,
        ) {
            dialog_title(c"Savestate");
            let text = CString::new(text.as_ref()).unwrap_or_default();
            centered_text(&text);
            ImGui::Spacing();
            const BAR_WIDTH: f32 = 440.0;
            let avail = ImGui::GetContentRegionAvail().x;
            if avail > BAR_WIDTH {
                ImGui::SetCursorPosX(ImGui::GetCursorPosX() + (avail - BAR_WIDTH) * 0.5);
            }
            let sz = ImVec2 { x: BAR_WIDTH, y: 28.0 };
            ImGui::ProgressBar(progress as f32 / 100.0, &sz, ptr::null());
            ImGui::EndPopup();
        }
        ImGui::OpenPopup(c"SavestateProgressPopup".as_ptr());

        ImGui::Render();
        ui_backend.render_draw_data(ImGui::GetDrawData());
        ui_backend.swap_window();
    }
}
