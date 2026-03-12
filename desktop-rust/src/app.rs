use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;

use eframe::egui::{
    self, Align, Color32, Context, CornerRadius, FontFamily, FontId, Key, Layout, Rect, RichText,
    ScrollArea, Stroke, TextureHandle, Ui, Vec2,
};
use rfd::FileDialog;

use crate::document::BinaryDocument;
use crate::filters::{DerivedView, FilterPipeline, FilterStep, build_derived_view};
use crate::viewer::{
    BIT_VALUE_NO_DATA, RowData, RowLayout, bit_offset_to_row, build_bit_window, build_row,
    build_row_layout,
};

const DEFAULT_ROW_WIDTH_BITS: usize = 128;
const MIN_ROW_WIDTH_BITS: usize = 8;
const MAX_ROW_WIDTH_BITS: usize = 4096;
const DEFAULT_BIT_SIZE: f32 = 7.0;
const MIN_BIT_SIZE: f32 = 2.0;
const MAX_BIT_SIZE: f32 = 24.0;
const ROW_WIDTH_STEP_BITS: usize = 1;
const BIT_SIZE_STEP: f32 = 1.0;
const TEXT_ROW_HEIGHT: f32 = 20.0;
const BIT_OVERSCAN_ROWS: usize = 24;
const BIT_OVERSCAN_COLS: usize = 32;
const RESIZE_BIT_OVERSCAN_ROWS: usize = 8;
const RESIZE_BIT_OVERSCAN_COLS: usize = 8;
const TEXT_OVERSCAN_ROWS: usize = 8;
const SCROLL_MULTIPLIER: Vec2 = Vec2::new(1.5, 2.5);
const BIT_ONE_COLOR: Color32 = Color32::from_rgb(32, 96, 246);
const BIT_ZERO_COLOR: Color32 = Color32::WHITE;
const BIT_BORDER_COLOR: Color32 = Color32::from_gray(200);
const BYTE_DIVIDER_COLOR: Color32 = Color32::from_rgb(220, 48, 48);
const APP_BG: Color32 = Color32::from_rgb(8, 12, 20);
const SIDEBAR_BG: Color32 = Color32::from_rgb(12, 18, 30);
const SURFACE_BG: Color32 = Color32::from_rgb(18, 24, 38);
const SURFACE_ALT_BG: Color32 = Color32::from_rgb(23, 31, 48);
const SURFACE_SUBTLE_BG: Color32 = Color32::from_rgb(28, 38, 58);
const BORDER_COLOR: Color32 = Color32::from_rgb(46, 62, 92);
const ACCENT_COLOR: Color32 = Color32::from_rgb(76, 183, 255);
const ACCENT_SOFT: Color32 = Color32::from_rgb(34, 82, 128);
const TEXT_PRIMARY: Color32 = Color32::from_rgb(236, 242, 255);
const TEXT_MUTED: Color32 = Color32::from_rgb(147, 163, 188);
const ERROR_COLOR: Color32 = Color32::from_rgb(255, 119, 119);
const HEX_COLUMN_MIN_WIDTH: f32 = 280.0;
const ASCII_COLUMN_MIN_WIDTH: f32 = 120.0;
const BIT_PANEL_MIN_WIDTH: f32 = 240.0;
const HEX_PANEL_DEFAULT_WIDTH: f32 = 360.0;
const POLL_INTERVAL: Duration = Duration::from_millis(50);
const SIDEBAR_WIDTH: f32 = 360.0;
const TEXT_CELL_PADDING_X: f32 = 10.0;

struct DerivedBuildResult {
    request_id: u64,
    result: Result<DerivedView, String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BitTextureKey {
    view_revision: u64,
    row_width_bits: usize,
    start_row: usize,
    row_count: usize,
    start_col: usize,
    col_count: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RowLayoutKey {
    view_revision: u64,
    row_width_bits: usize,
}

struct CachedRowLayout {
    key: RowLayoutKey,
    layout: Arc<RowLayout>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TextRowCacheKey {
    view_revision: u64,
    row_width_bits: usize,
    start_row: usize,
    row_count: usize,
}

struct CachedTextRows {
    key: TextRowCacheKey,
    rows: Vec<RowData>,
}

pub struct BitViewerApp {
    document: Option<BinaryDocument>,
    derived_view: Option<DerivedView>,
    pipeline: FilterPipeline,
    show_text_pane: bool,
    bit_texture: Option<TextureHandle>,
    bit_texture_key: Option<BitTextureKey>,
    row_layout_cache: Option<CachedRowLayout>,
    text_row_cache: Option<CachedTextRows>,
    derived_view_revision: u64,
    row_width_bits: usize,
    target_row_width_bits: usize,
    bit_size: f32,
    target_bit_size: f32,
    jump_bit_input: String,
    jump_byte_input: String,
    path_input: String,
    pending_bit_scroll_to_row: Option<usize>,
    pending_text_scroll_to_row: Option<usize>,
    current_bit_scroll_row: usize,
    current_text_scroll_row: usize,
    file_dialog_rx: Option<Receiver<Option<PathBuf>>>,
    file_dialog_pending: bool,
    rebuild_rx: Option<Receiver<DerivedBuildResult>>,
    rebuild_pending: bool,
    rebuild_request_id: u64,
    show_shortcuts: bool,
    last_error: Option<String>,
}

impl Default for BitViewerApp {
    fn default() -> Self {
        Self {
            document: None,
            derived_view: None,
            pipeline: FilterPipeline::default(),
            show_text_pane: true,
            bit_texture: None,
            bit_texture_key: None,
            row_layout_cache: None,
            text_row_cache: None,
            derived_view_revision: 0,
            row_width_bits: DEFAULT_ROW_WIDTH_BITS,
            target_row_width_bits: DEFAULT_ROW_WIDTH_BITS,
            bit_size: DEFAULT_BIT_SIZE,
            target_bit_size: DEFAULT_BIT_SIZE,
            jump_bit_input: String::new(),
            jump_byte_input: String::new(),
            path_input: String::new(),
            pending_bit_scroll_to_row: None,
            pending_text_scroll_to_row: None,
            current_bit_scroll_row: 0,
            current_text_scroll_row: 0,
            file_dialog_rx: None,
            file_dialog_pending: false,
            rebuild_rx: None,
            rebuild_pending: false,
            rebuild_request_id: 0,
            show_shortcuts: false,
            last_error: None,
        }
    }
}

impl BitViewerApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        Self::configure_theme(&cc.egui_ctx);
        Self::default()
    }

    fn configure_theme(context: &Context) {
        let mut style = (*context.style()).clone();
        style.spacing.item_spacing = egui::vec2(12.0, 12.0);
        style.spacing.window_margin = egui::Margin::same(18);
        style.spacing.menu_margin = egui::Margin::same(14);
        style.spacing.button_padding = egui::vec2(14.0, 10.0);
        style.spacing.interact_size = egui::vec2(44.0, 34.0);
        style.spacing.slider_width = 180.0;
        style.spacing.text_edit_width = 220.0;
        style.spacing.default_area_size = egui::vec2(720.0, 480.0);

        style.text_styles = [
            (
                egui::TextStyle::Small,
                FontId::new(11.0, FontFamily::Proportional),
            ),
            (
                egui::TextStyle::Body,
                FontId::new(14.0, FontFamily::Proportional),
            ),
            (
                egui::TextStyle::Button,
                FontId::new(14.0, FontFamily::Proportional),
            ),
            (
                egui::TextStyle::Heading,
                FontId::new(24.0, FontFamily::Proportional),
            ),
            (
                egui::TextStyle::Monospace,
                FontId::new(13.0, FontFamily::Monospace),
            ),
        ]
        .into();

        let mut visuals = egui::Visuals::dark();
        visuals.override_text_color = Some(TEXT_PRIMARY);
        visuals.hyperlink_color = ACCENT_COLOR;
        visuals.faint_bg_color = SURFACE_ALT_BG;
        visuals.extreme_bg_color = Color32::from_rgb(14, 19, 30);
        visuals.code_bg_color = SURFACE_ALT_BG;
        visuals.panel_fill = APP_BG;
        visuals.window_fill = SURFACE_BG;
        visuals.window_stroke = Stroke::new(1.0, BORDER_COLOR);
        visuals.window_corner_radius = CornerRadius::same(20);
        visuals.menu_corner_radius = CornerRadius::same(16);
        visuals.window_shadow = egui::epaint::Shadow {
            offset: [0, 18],
            blur: 36,
            spread: 0,
            color: Color32::from_black_alpha(96),
        };
        visuals.popup_shadow = egui::epaint::Shadow {
            offset: [0, 14],
            blur: 28,
            spread: 0,
            color: Color32::from_black_alpha(92),
        };
        visuals.selection.bg_fill = ACCENT_SOFT;
        visuals.selection.stroke = Stroke::new(1.0, ACCENT_COLOR);
        visuals.widgets.noninteractive.bg_fill = SURFACE_BG;
        visuals.widgets.noninteractive.weak_bg_fill = SURFACE_BG;
        visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER_COLOR);
        visuals.widgets.noninteractive.corner_radius = CornerRadius::same(16);
        visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT_MUTED);
        visuals.widgets.inactive.bg_fill = SURFACE_ALT_BG;
        visuals.widgets.inactive.weak_bg_fill = SURFACE_ALT_BG;
        visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER_COLOR);
        visuals.widgets.inactive.corner_radius = CornerRadius::same(14);
        visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT_PRIMARY);
        visuals.widgets.hovered.bg_fill = Color32::from_rgb(33, 44, 68);
        visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(33, 44, 68);
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT_COLOR);
        visuals.widgets.hovered.corner_radius = CornerRadius::same(16);
        visuals.widgets.hovered.fg_stroke = Stroke::new(1.2, TEXT_PRIMARY);
        visuals.widgets.active.bg_fill = ACCENT_SOFT;
        visuals.widgets.active.weak_bg_fill = ACCENT_SOFT;
        visuals.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT_COLOR);
        visuals.widgets.active.corner_radius = CornerRadius::same(16);
        visuals.widgets.active.fg_stroke = Stroke::new(1.3, TEXT_PRIMARY);
        visuals.widgets.open.bg_fill = SURFACE_SUBTLE_BG;
        visuals.widgets.open.weak_bg_fill = SURFACE_SUBTLE_BG;
        visuals.widgets.open.bg_stroke = Stroke::new(1.0, ACCENT_SOFT);
        visuals.widgets.open.corner_radius = CornerRadius::same(16);
        visuals.widgets.open.fg_stroke = Stroke::new(1.0, TEXT_PRIMARY);
        visuals.warn_fg_color = Color32::from_rgb(255, 190, 92);
        visuals.error_fg_color = ERROR_COLOR;
        visuals.indent_has_left_vline = false;

        style.visuals = visuals;
        context.set_style(style);
    }
}

impl eframe::App for BitViewerApp {
    fn update(&mut self, context: &Context, _frame: &mut eframe::Frame) {
        self.poll_file_dialog();
        self.poll_rebuild();
        self.handle_file_drop(context);
        self.handle_keyboard_shortcuts(context);
        self.advance_view_settings(context);

        if self.file_dialog_pending || self.rebuild_pending {
            context.request_repaint_after(POLL_INTERVAL);
        }

        egui::SidePanel::left("sidebar")
            .default_width(SIDEBAR_WIDTH)
            .min_width(300.0)
            .resizable(true)
            .frame(
                egui::Frame::new()
                    .fill(SIDEBAR_BG)
                    .stroke(Stroke::new(1.0, BORDER_COLOR))
                    .inner_margin(18),
            )
            .show(context, |ui| {
                self.show_sidebar(ui);
            });

        egui::TopBottomPanel::bottom("status_bar")
            .frame(
                egui::Frame::new()
                    .fill(SURFACE_BG)
                    .stroke(Stroke::new(1.0, BORDER_COLOR))
                    .inner_margin(12),
            )
            .show(context, |ui| {
                self.show_status(ui);
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(APP_BG).inner_margin(20))
            .show(context, |ui| {
                if self.document.is_some() {
                    self.show_main_content(ui);
                } else {
                    self.show_empty_state(ui);
                }
            });

        self.show_shortcuts_window(context);
    }
}

impl BitViewerApp {
    fn invalidate_render_caches(&mut self) {
        self.bit_texture = None;
        self.bit_texture_key = None;
        self.row_layout_cache = None;
        self.text_row_cache = None;
    }

    fn ensure_row_layout(&mut self) -> Option<Arc<RowLayout>> {
        let view = self.derived_view.as_ref()?;
        let key = RowLayoutKey {
            view_revision: self.derived_view_revision,
            row_width_bits: self.row_width_bits,
        };

        if self.row_layout_cache.as_ref().map(|cached| cached.key) != Some(key) {
            self.row_layout_cache = Some(CachedRowLayout {
                key,
                layout: Arc::new(build_row_layout(view, self.row_width_bits)),
            });
            self.text_row_cache = None;
        }

        self.row_layout_cache
            .as_ref()
            .map(|cached| Arc::clone(&cached.layout))
    }

    fn text_rows(&mut self, layout: &RowLayout, start_row: usize, row_count: usize) -> &[RowData] {
        let key = TextRowCacheKey {
            view_revision: self.derived_view_revision,
            row_width_bits: self.row_width_bits,
            start_row,
            row_count,
        };

        if self.text_row_cache.as_ref().map(|cached| cached.key) != Some(key) {
            let rows = self
                .derived_view
                .as_ref()
                .map(|view| {
                    (0..row_count)
                        .map(|row_offset| build_row(view, layout, start_row + row_offset))
                        .collect()
                })
                .unwrap_or_default();
            self.text_row_cache = Some(CachedTextRows { key, rows });
        }

        self.text_row_cache
            .as_ref()
            .map(|cached| cached.rows.as_slice())
            .unwrap_or(&[])
    }

    fn show_sidebar(&mut self, ui: &mut Ui) {
        ui.heading(RichText::new("Bit Viewer").color(TEXT_PRIMARY));
        ui.label(
            RichText::new("High-speed binary inspection for large local files.").color(TEXT_MUTED),
        );
        ui.add_space(8.0);

        ScrollArea::vertical()
            .id_salt("sidebar-scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                self.card_frame(SURFACE_BG).show(ui, |ui| {
                    self.section_header(
                        ui,
                        "Workspace",
                        "Open a file, inspect shortcuts, or resume from a pasted path.",
                    );

                    ui.horizontal(|ui| {
                        if ui
                            .add_sized([120.0, 36.0], egui::Button::new("Open file"))
                            .clicked()
                        {
                            self.start_file_dialog();
                        }
                        if ui
                            .add_sized([84.0, 36.0], egui::Button::new("Info"))
                            .clicked()
                        {
                            self.show_shortcuts = true;
                        }
                    });

                    if self.file_dialog_pending {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(RichText::new("Opening chooser...").color(TEXT_MUTED));
                        });
                    }

                    if self.rebuild_pending {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(
                                RichText::new("Rebuilding filtered view...").color(TEXT_MUTED),
                            );
                        });
                    }

                    ui.separator();
                    ui.label(RichText::new("File path").color(TEXT_MUTED));
                    let path_response = ui.text_edit_singleline(&mut self.path_input);
                    if path_response.lost_focus() && ui.input(|input| input.key_pressed(Key::Enter))
                    {
                        self.open_path_input();
                    }
                    if ui
                        .add_sized([ui.available_width(), 34.0], egui::Button::new("Open path"))
                        .clicked()
                    {
                        self.open_path_input();
                    }
                });

                self.card_frame(SURFACE_BG).show(ui, |ui| {
                    self.section_header(
                        ui,
                        "Viewer",
                        "Tune density, scale, and visibility without changing the grid renderer.",
                    );

                    egui::Grid::new("viewer-settings-grid")
                        .num_columns(2)
                        .spacing(egui::vec2(12.0, 10.0))
                        .show(ui, |ui| {
                            ui.label(RichText::new("Row width").color(TEXT_MUTED));
                            let row_width_response = ui.add(
                                egui::DragValue::new(&mut self.target_row_width_bits)
                                    .range(MIN_ROW_WIDTH_BITS..=MAX_ROW_WIDTH_BITS)
                                    .speed(1.0),
                            );
                            if row_width_response.changed() {
                                self.target_row_width_bits = self
                                    .target_row_width_bits
                                    .clamp(MIN_ROW_WIDTH_BITS, MAX_ROW_WIDTH_BITS);
                                self.row_width_bits = self.target_row_width_bits;
                                self.invalidate_render_caches();
                            }
                            ui.end_row();

                            ui.label(RichText::new("Bit size").color(TEXT_MUTED));
                            let bit_size_response = ui.add(
                                egui::Slider::new(
                                    &mut self.target_bit_size,
                                    MIN_BIT_SIZE..=MAX_BIT_SIZE,
                                )
                                .clamping(egui::SliderClamping::Always)
                                .step_by(1.0),
                            );
                            if bit_size_response.changed() {
                                self.target_bit_size =
                                    self.target_bit_size.clamp(MIN_BIT_SIZE, MAX_BIT_SIZE);
                                self.bit_size = self.target_bit_size;
                            }
                            ui.end_row();
                        });
                    self.target_row_width_bits = self
                        .target_row_width_bits
                        .clamp(MIN_ROW_WIDTH_BITS, MAX_ROW_WIDTH_BITS);
                    self.bit_size = self.bit_size.clamp(MIN_BIT_SIZE, MAX_BIT_SIZE);
                    self.target_bit_size = self.target_bit_size.clamp(MIN_BIT_SIZE, MAX_BIT_SIZE);

                    if self.row_width_bits != self.target_row_width_bits {
                        ui.label(
                            RichText::new(format!(
                                "Animating width {} -> {}",
                                self.row_width_bits, self.target_row_width_bits
                            ))
                            .small()
                            .color(TEXT_MUTED),
                        );
                    }

                    ui.separator();
                    ui.checkbox(&mut self.show_text_pane, "Show hex / ASCII pane");
                });

                self.card_frame(SURFACE_BG).show(ui, |ui| {
                    self.section_header(
                        ui,
                        "Navigation",
                        "Jump directly to offsets and keep both panes aligned.",
                    );

                    ui.label(RichText::new("Jump to byte offset").color(TEXT_MUTED));
                    let byte_response = ui.text_edit_singleline(&mut self.jump_byte_input);
                    if byte_response.lost_focus() && ui.input(|input| input.key_pressed(Key::Enter))
                    {
                        self.jump_to_byte();
                    }
                    if ui
                        .add_sized(
                            [ui.available_width(), 34.0],
                            egui::Button::new("Jump to byte"),
                        )
                        .clicked()
                    {
                        self.jump_to_byte();
                    }

                    ui.separator();

                    ui.label(RichText::new("Jump to bit offset").color(TEXT_MUTED));
                    let bit_response = ui.text_edit_singleline(&mut self.jump_bit_input);
                    if bit_response.lost_focus() && ui.input(|input| input.key_pressed(Key::Enter))
                    {
                        self.jump_to_bit();
                    }
                    if ui
                        .add_sized(
                            [ui.available_width(), 34.0],
                            egui::Button::new("Jump to bit"),
                        )
                        .clicked()
                    {
                        self.jump_to_bit();
                    }
                });

                let rebuild_requested = self
                    .card_frame(SURFACE_BG)
                    .show(ui, |ui| self.show_filter_editor(ui))
                    .inner;

                if rebuild_requested {
                    self.schedule_rebuild();
                }

                if let Some(error) = &self.last_error {
                    self.card_frame(Color32::from_rgb(58, 22, 28))
                        .show(ui, |ui| {
                            self.section_header(
                                ui,
                                "Issue",
                                "The current action did not complete.",
                            );
                            ui.colored_label(ERROR_COLOR, error);
                        });
                }
            });
    }

    fn show_main_content(&mut self, ui: &mut Ui) {
        self.card_frame(SURFACE_BG).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                let file_label = self
                    .document
                    .as_ref()
                    .map(|document| document.file_name().to_owned())
                    .unwrap_or_else(|| "No file open".to_owned());
                ui.heading(RichText::new(file_label).color(TEXT_PRIMARY));
                ui.add_space(6.0);
                self.status_chip(
                    ui,
                    if self.rebuild_pending {
                        "Processing filters"
                    } else {
                        "Ready"
                    },
                );
            });
        });

        ui.add_space(16.0);

        self.card_frame(SURFACE_ALT_BG).show(ui, |ui| {
            self.show_viewer(ui);
        });
    }

    fn show_filter_editor(&mut self, ui: &mut Ui) -> bool {
        let visuals = &mut ui.style_mut().visuals;
        visuals.override_text_color = Some(TEXT_PRIMARY);
        visuals.widgets.inactive.bg_fill = SURFACE_SUBTLE_BG;
        visuals.widgets.inactive.weak_bg_fill = SURFACE_SUBTLE_BG;
        visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT_PRIMARY);
        visuals.widgets.hovered.bg_fill = Color32::from_rgb(37, 50, 76);
        visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(37, 50, 76);
        visuals.widgets.hovered.fg_stroke = Stroke::new(1.1, TEXT_PRIMARY);
        visuals.widgets.active.bg_fill = Color32::from_rgb(43, 59, 88);
        visuals.widgets.active.weak_bg_fill = Color32::from_rgb(43, 59, 88);
        visuals.widgets.active.fg_stroke = Stroke::new(1.2, TEXT_PRIMARY);

        let mut changed = false;
        let mut move_up = None;
        let mut move_down = None;
        let mut delete = None;

        self.section_header(
            ui,
            "Filter Pipeline",
            "Stack filters in order. Group-aware steps only work after a sync or grouping stage.",
        );

        if self.pipeline.is_empty() {
            ui.label(
                RichText::new("No filters. The whole file is shown as one continuous stream.")
                    .small()
                    .color(TEXT_MUTED),
            );
        }

        for index in 0..self.pipeline.steps.len() {
            self.card_frame(SURFACE_ALT_BG).show(ui, |ui| {
                ui.horizontal(|ui| {
                    self.status_chip(ui, &format!("{:02}", index + 1));
                    ui.label(RichText::new(self.pipeline.steps[index].label()).strong());
                });

                ui.add_space(4.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.small_button("Delete").clicked() {
                        delete = Some(index);
                    }
                    if ui.small_button("Down").clicked() && index + 1 < self.pipeline.steps.len() {
                        move_down = Some(index);
                    }
                    if ui.small_button("Up").clicked() && index > 0 {
                        move_up = Some(index);
                    }
                });

                ui.add_space(6.0);

                let step = &mut self.pipeline.steps[index];
                match step {
                    FilterStep::SyncOnPreamble { bits } => {
                        let response = ui.horizontal(|ui| {
                            ui.label(RichText::new("Preamble bits").color(TEXT_MUTED));
                            ui.add_space(6.0);
                            ui.scope(|ui| {
                                let visuals = &mut ui.style_mut().visuals;
                                visuals.override_text_color = Some(Color32::BLACK);
                                ui.add_sized(
                                    [ui.available_width(), 28.0],
                                    egui::TextEdit::singleline(bits),
                                )
                            })
                        });
                        if response.inner.inner.changed() {
                            changed = true;
                        }
                    }
                    FilterStep::ReverseBitsPerByte
                    | FilterStep::InvertBits
                    | FilterStep::Flatten => {
                        ui.label(
                            RichText::new("This step has no additional parameters.")
                                .small()
                                .color(TEXT_MUTED),
                        );
                    }
                    FilterStep::XorMask { mask } => {
                        egui::Grid::new(("xor-grid", index))
                            .num_columns(2)
                            .spacing(egui::vec2(12.0, 10.0))
                            .show(ui, |ui| {
                                ui.label(RichText::new("Mask").color(TEXT_MUTED));
                                if ui
                                    .add(
                                        egui::DragValue::new(mask)
                                            .range(0..=u8::MAX)
                                            .speed(1.0)
                                            .prefix("0x"),
                                    )
                                    .changed()
                                {
                                    changed = true;
                                }
                                ui.end_row();
                            });
                    }
                    FilterStep::KeepGroupsLongerThanBytes { min_bytes } => {
                        egui::Grid::new(("keep-groups-grid", index))
                            .num_columns(2)
                            .spacing(egui::vec2(12.0, 10.0))
                            .show(ui, |ui| {
                                ui.label(RichText::new("Minimum bytes").color(TEXT_MUTED));
                                if ui
                                    .add(
                                        egui::DragValue::new(min_bytes)
                                            .range(0..=usize::MAX)
                                            .speed(1.0),
                                    )
                                    .changed()
                                {
                                    changed = true;
                                }
                                ui.end_row();
                            });
                    }
                    FilterStep::SelectBitRangeFromGroup {
                        start_bit,
                        length_bits,
                    } => {
                        egui::Grid::new(("select-range-grid", index))
                            .num_columns(2)
                            .spacing(egui::vec2(12.0, 10.0))
                            .show(ui, |ui| {
                                ui.label(RichText::new("Start bit").color(TEXT_MUTED));
                                if ui
                                    .add(
                                        egui::DragValue::new(start_bit)
                                            .range(0..=usize::MAX)
                                            .speed(1.0),
                                    )
                                    .changed()
                                {
                                    changed = true;
                                }
                                ui.end_row();

                                ui.label(RichText::new("Length bits").color(TEXT_MUTED));
                                if ui
                                    .add(
                                        egui::DragValue::new(length_bits)
                                            .range(0..=usize::MAX)
                                            .speed(1.0),
                                    )
                                    .changed()
                                {
                                    changed = true;
                                }
                                ui.end_row();
                            });
                    }
                }
            });
        }

        if let Some(index) = move_up {
            self.pipeline.steps.swap(index - 1, index);
            changed = true;
        }
        if let Some(index) = move_down {
            self.pipeline.steps.swap(index, index + 1);
            changed = true;
        }
        if let Some(index) = delete {
            self.pipeline.steps.remove(index);
            changed = true;
        }

        ui.separator();
        ui.label(RichText::new("Add step").color(TEXT_MUTED));
        egui::Grid::new("filter-add-grid")
            .num_columns(2)
            .spacing(egui::vec2(10.0, 10.0))
            .show(ui, |ui| {
                if ui.button("Sync on preamble").clicked() {
                    self.pipeline.steps.push(FilterStep::SyncOnPreamble {
                        bits: "1010".to_owned(),
                    });
                    changed = true;
                }
                if ui.button("Reverse bytes").clicked() {
                    self.pipeline.steps.push(FilterStep::ReverseBitsPerByte);
                    changed = true;
                }
                ui.end_row();

                if ui.button("Invert bits").clicked() {
                    self.pipeline.steps.push(FilterStep::InvertBits);
                    changed = true;
                }
                if ui.button("Flatten").clicked() {
                    self.pipeline.steps.push(FilterStep::Flatten);
                    changed = true;
                }
                ui.end_row();

                if ui.button("XOR mask").clicked() {
                    self.pipeline.steps.push(FilterStep::XorMask { mask: 0xFF });
                    changed = true;
                }
                if ui.button("Keep groups > N bytes").clicked() {
                    self.pipeline
                        .steps
                        .push(FilterStep::KeepGroupsLongerThanBytes { min_bytes: 6 });
                    changed = true;
                }
                if ui.button("Select bit range").clicked() {
                    self.pipeline
                        .steps
                        .push(FilterStep::SelectBitRangeFromGroup {
                            start_bit: 0,
                            length_bits: 48,
                        });
                    changed = true;
                }
                ui.end_row();
            });

        if ui
            .add_sized(
                [ui.available_width(), 34.0],
                egui::Button::new("Clear pipeline"),
            )
            .clicked()
        {
            self.pipeline.steps.clear();
            changed = true;
        }

        changed
    }

    fn show_status(&self, ui: &mut Ui) {
        ui.horizontal_wrapped(|ui| {
            if let Some(document) = &self.document {
                self.status_chip(ui, document.file_name());
                self.status_chip(ui, &format!("source {} bytes", document.len_bytes()));
                self.status_chip(ui, &format!("{} bits", document.len_bits()));

                if let Some(view) = &self.derived_view {
                    self.status_chip(ui, &format!("{} groups", view.group_count()));
                    self.status_chip(ui, &format!("derived {} bits", view.total_bits()));
                    self.status_chip(
                        ui,
                        &format!("{} bytes rounded", view.total_bytes_rounded_up()),
                    );
                }

                if self.rebuild_pending {
                    ui.spinner();
                    ui.label(RichText::new("processing filters").color(TEXT_MUTED));
                }

                if !self.pipeline.is_empty() {
                    self.status_chip(ui, &format!("{} filter(s)", self.pipeline.steps.len()));
                }

                ui.label(
                    RichText::new(document.path().display().to_string())
                        .monospace()
                        .small()
                        .color(TEXT_MUTED),
                );
            } else {
                ui.label(
                    RichText::new("Open a file to start exploring bits, hex, and ASCII.")
                        .color(TEXT_MUTED),
                );
            }
        });
    }

    fn show_empty_state(&mut self, ui: &mut Ui) {
        ui.centered_and_justified(|ui| {
            self.card_frame(SURFACE_BG).show(ui, |ui| {
                ui.set_max_width(520.0);
                ui.vertical_centered(|ui| {
                    ui.heading(RichText::new("Bit Viewer Desktop").color(TEXT_PRIMARY));
                    ui.label(
                        RichText::new("A native Rust binary explorer with a faster rendering path for large files.")
                            .color(TEXT_MUTED),
                    );
                    ui.add_space(8.0);
                    if ui
                        .add_sized([160.0, 38.0], egui::Button::new("Open file"))
                        .clicked()
                    {
                        self.start_file_dialog();
                    }
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(
                            "If the native chooser stalls, paste a full path in the left panel or drag a file into the window.",
                        )
                        .color(TEXT_MUTED),
                    );
                });
            });
        });
    }

    fn show_shortcuts_window(&mut self, context: &Context) {
        if !self.show_shortcuts {
            return;
        }

        let mut show_shortcuts = self.show_shortcuts;
        egui::Window::new("Shortcuts")
            .open(&mut show_shortcuts)
            .resizable(false)
            .collapsible(false)
            .frame(
                egui::Frame::new()
                    .fill(SURFACE_BG)
                    .stroke(Stroke::new(1.0, BORDER_COLOR))
                    .corner_radius(CornerRadius::same(20))
                    .inner_margin(18),
            )
            .show(context, |ui| {
                self.section_header(
                    ui,
                    "Shortcuts",
                    "Fast controls for density, size, and viewport movement.",
                );
                egui::Grid::new("shortcuts-grid")
                    .num_columns(2)
                    .spacing(egui::vec2(16.0, 10.0))
                    .show(ui, |ui| {
                        ui.monospace("[ / ]");
                        ui.label(RichText::new("Decrease / increase row width").color(TEXT_MUTED));
                        ui.end_row();

                        ui.monospace("- / =");
                        ui.label(RichText::new("Decrease / increase bit size").color(TEXT_MUTED));
                        ui.end_row();

                        ui.monospace("h");
                        ui.label(RichText::new("Toggle hex / ASCII panes").color(TEXT_MUTED));
                        ui.end_row();

                        ui.monospace("i");
                        ui.label(RichText::new("Toggle this shortcuts window").color(TEXT_MUTED));
                        ui.end_row();
                    });
                ui.separator();
                ui.label(RichText::new("Navigation").strong());
                egui::Grid::new("navigation-shortcuts-grid")
                    .num_columns(2)
                    .spacing(egui::vec2(16.0, 10.0))
                    .show(ui, |ui| {
                        ui.monospace("Arrow Up / Down");
                        ui.label(RichText::new("Scroll by 1 row").color(TEXT_MUTED));
                        ui.end_row();

                        ui.monospace("Page Up / Down");
                        ui.label(RichText::new("Scroll by 20 rows").color(TEXT_MUTED));
                        ui.end_row();

                        ui.monospace("Home / End");
                        ui.label(RichText::new("Jump to start / end").color(TEXT_MUTED));
                        ui.end_row();
                    });
            });
        self.show_shortcuts = show_shortcuts;
    }

    fn card_frame(&self, fill: Color32) -> egui::Frame {
        egui::Frame::new()
            .fill(fill)
            .stroke(Stroke::new(1.0, BORDER_COLOR))
            .corner_radius(CornerRadius::same(18))
            .inner_margin(18)
    }

    fn section_header(&self, ui: &mut Ui, title: &str, subtitle: &str) {
        ui.label(RichText::new(title).strong().color(TEXT_PRIMARY));
        ui.label(RichText::new(subtitle).small().color(TEXT_MUTED));
        ui.add_space(4.0);
    }

    fn status_chip(&self, ui: &mut Ui, label: &str) {
        egui::Frame::new()
            .fill(SURFACE_SUBTLE_BG)
            .stroke(Stroke::new(1.0, ACCENT_SOFT))
            .corner_radius(CornerRadius::same(255))
            .inner_margin(egui::Margin::symmetric(10, 6))
            .show(ui, |ui| {
                ui.label(RichText::new(label).small().color(TEXT_PRIMARY));
            });
    }

    fn show_viewer(&mut self, ui: &mut Ui) {
        if self.derived_view.is_none() {
            ui.centered_and_justified(|ui| {
                if self.rebuild_pending {
                    ui.spinner();
                    ui.label(RichText::new("Building filtered view...").color(TEXT_MUTED));
                } else {
                    ui.label(RichText::new("No derived view is available.").color(TEXT_MUTED));
                }
            });
            return;
        }

        let Some(layout) = self.ensure_row_layout() else {
            return;
        };
        let total_rows = layout.total_rows();
        if total_rows == 0 {
            ui.centered_and_justified(|ui| {
                ui.label(
                    RichText::new("No rows remain after applying the current filters.")
                        .color(TEXT_MUTED),
                );
            });
            return;
        }

        let pending_bit_scroll_to_row = self.pending_bit_scroll_to_row.take();
        let pending_text_scroll_to_row = self.pending_text_scroll_to_row.take();
        let bit_row_height = self.bit_size;
        let text_row_height = TEXT_ROW_HEIGHT;
        let bytes_per_row = self.row_width_bits.div_ceil(8);
        let hex_width = HEX_COLUMN_MIN_WIDTH.max(bytes_per_row as f32 * 21.0);
        let bit_panel_width = self.row_width_bits as f32 * self.bit_size;
        let bit_content_height = total_rows as f32 * bit_row_height;
        let available_height = ui.available_height();
        let text_scroll_target_row =
            pending_text_scroll_to_row.unwrap_or(self.current_text_scroll_row);
        let resizing_row_width = self.row_width_bits != self.target_row_width_bits;
        let bit_overscan_rows = if resizing_row_width {
            RESIZE_BIT_OVERSCAN_ROWS
        } else {
            BIT_OVERSCAN_ROWS
        };
        let bit_overscan_cols = if resizing_row_width {
            RESIZE_BIT_OVERSCAN_COLS
        } else {
            BIT_OVERSCAN_COLS
        };
        let mut observed_bit_scroll_row = self.current_bit_scroll_row;
        let mut observed_text_scroll_row = self.current_text_scroll_row;
        if self.show_text_pane {
            let mut hex_observed_row = observed_text_scroll_row;
            let mut ascii_observed_row = observed_text_scroll_row;

            egui::SidePanel::left("bit-grid-pane")
                .resizable(true)
                .show_separator_line(true)
                .default_width(bit_panel_width.max(BIT_PANEL_MIN_WIDTH))
                .min_width(BIT_PANEL_MIN_WIDTH)
                .frame(egui::Frame::new().inner_margin(egui::Margin::symmetric(0, 0)))
                .show_inside(ui, |ui| {
                    ui.set_min_height(available_height);
                    ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);

                    let mut scroll_area = ScrollArea::both()
                        .id_salt("native-bit-scroll")
                        .auto_shrink([false, false])
                        .max_height(available_height)
                        .min_scrolled_height(available_height)
                        .wheel_scroll_multiplier(SCROLL_MULTIPLIER);

                    if let Some(target_row) = pending_bit_scroll_to_row {
                        scroll_area =
                            scroll_area.vertical_scroll_offset(target_row as f32 * bit_row_height);
                    }

                    let output = scroll_area.show_viewport(ui, |ui, viewport| {
                        ui.set_min_size(Vec2::new(bit_panel_width, bit_content_height));

                        let viewport_start_row =
                            (viewport.min.y / bit_row_height).floor().max(0.0) as usize;
                        let viewport_end_row = (viewport.max.y / bit_row_height).ceil() as usize;
                        let viewport_start_col =
                            (viewport.min.x / self.bit_size).floor().max(0.0) as usize;
                        let viewport_end_col =
                            (viewport.max.x / self.bit_size).ceil().max(0.0) as usize;
                        let cache_start_row = viewport_start_row.saturating_sub(bit_overscan_rows);
                        let cache_end_row = (viewport_end_row + bit_overscan_rows).min(total_rows);
                        let cache_row_count = cache_end_row.saturating_sub(cache_start_row);
                        let cache_start_col = viewport_start_col.saturating_sub(bit_overscan_cols);
                        let cache_end_col =
                            (viewport_end_col + bit_overscan_cols).min(self.row_width_bits);
                        let cache_col_count = cache_end_col.saturating_sub(cache_start_col);

                        if let Some(texture_id) = self.ensure_bit_texture(
                            ui.ctx(),
                            layout.as_ref(),
                            cache_start_row,
                            cache_row_count,
                            cache_start_col,
                            cache_col_count,
                        ) {
                            let cache_rect = Rect::from_min_size(
                                egui::pos2(
                                    ui.max_rect().left() + cache_start_col as f32 * self.bit_size,
                                    ui.max_rect().top() + cache_start_row as f32 * bit_row_height,
                                ),
                                Vec2::new(
                                    cache_col_count as f32 * self.bit_size,
                                    cache_row_count as f32 * bit_row_height,
                                ),
                            );
                            ui.painter().image(
                                texture_id,
                                cache_rect,
                                Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                                Color32::WHITE,
                            );
                            paint_bit_grid_lines(
                                ui,
                                cache_rect,
                                cache_start_col,
                                cache_col_count,
                                cache_row_count,
                                self.bit_size,
                                bit_row_height,
                            );
                        }
                    });
                    observed_bit_scroll_row =
                        (output.state.offset.y / bit_row_height).floor().max(0.0) as usize;
                });

            egui::SidePanel::left("hex-text-pane")
                .resizable(true)
                .show_separator_line(true)
                .default_width(hex_width.max(HEX_PANEL_DEFAULT_WIDTH))
                .min_width(HEX_COLUMN_MIN_WIDTH)
                .frame(
                    egui::Frame::new()
                        .fill(SURFACE_ALT_BG)
                        .inner_margin(egui::Margin::symmetric(12, 8)),
                )
                .show_inside(ui, |ui| {
                    ui.set_min_height(available_height);
                    let pane_width = ui.available_width().max(HEX_COLUMN_MIN_WIDTH);
                    let mut scroll_area = ScrollArea::both()
                        .id_salt("native-hex-scroll")
                        .auto_shrink([false, false])
                        .max_height(available_height)
                        .min_scrolled_height(available_height)
                        .wheel_scroll_multiplier(SCROLL_MULTIPLIER);

                    scroll_area = scroll_area
                        .vertical_scroll_offset(text_scroll_target_row as f32 * text_row_height);

                    let output =
                        scroll_area.show_rows(ui, text_row_height, total_rows, |ui, row_range| {
                            let start = row_range.start.saturating_sub(TEXT_OVERSCAN_ROWS);
                            let end = (row_range.end + TEXT_OVERSCAN_ROWS).min(total_rows);

                            for row in
                                self.text_rows(layout.as_ref(), start, end.saturating_sub(start))
                            {
                                paint_single_text_row(
                                    ui,
                                    &row.hex,
                                    text_row_height,
                                    pane_width.max(hex_width),
                                    TEXT_PRIMARY,
                                );
                            }
                        });
                    hex_observed_row =
                        (output.state.offset.y / text_row_height).floor().max(0.0) as usize;
                });

            egui::CentralPanel::default()
                .frame(
                    egui::Frame::new()
                        .fill(SURFACE_ALT_BG)
                        .inner_margin(egui::Margin::symmetric(20, 8)),
                )
                .show_inside(ui, |ui| {
                    ui.set_min_height(available_height);
                    let pane_width = ui.available_width().max(ASCII_COLUMN_MIN_WIDTH);
                    let mut scroll_area = ScrollArea::both()
                        .id_salt("native-ascii-scroll")
                        .auto_shrink([false, false])
                        .max_height(available_height)
                        .min_scrolled_height(available_height)
                        .wheel_scroll_multiplier(SCROLL_MULTIPLIER);

                    scroll_area = scroll_area
                        .vertical_scroll_offset(text_scroll_target_row as f32 * text_row_height);

                    let output =
                        scroll_area.show_rows(ui, text_row_height, total_rows, |ui, row_range| {
                            let start = row_range.start.saturating_sub(TEXT_OVERSCAN_ROWS);
                            let end = (row_range.end + TEXT_OVERSCAN_ROWS).min(total_rows);

                            for row in
                                self.text_rows(layout.as_ref(), start, end.saturating_sub(start))
                            {
                                paint_single_text_row(
                                    ui,
                                    &row.ascii,
                                    text_row_height,
                                    pane_width,
                                    TEXT_MUTED,
                                );
                            }
                        });
                    ascii_observed_row =
                        (output.state.offset.y / text_row_height).floor().max(0.0) as usize;
                });

            observed_text_scroll_row = if hex_observed_row != text_scroll_target_row {
                hex_observed_row
            } else {
                ascii_observed_row
            };

            if observed_text_scroll_row != text_scroll_target_row {
                ui.ctx().request_repaint();
            }
        } else {
            egui::CentralPanel::default()
                .frame(
                    egui::Frame::new()
                        .fill(SURFACE_ALT_BG)
                        .inner_margin(egui::Margin::symmetric(0, 0)),
                )
                .show_inside(ui, |ui| {
                    ui.set_min_height(available_height);
                    ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);

                    let mut scroll_area = ScrollArea::both()
                        .id_salt("native-bit-scroll")
                        .auto_shrink([false, false])
                        .max_height(available_height)
                        .min_scrolled_height(available_height)
                        .wheel_scroll_multiplier(SCROLL_MULTIPLIER);

                    if let Some(target_row) = pending_bit_scroll_to_row {
                        scroll_area =
                            scroll_area.vertical_scroll_offset(target_row as f32 * bit_row_height);
                    }

                    let output = scroll_area.show_viewport(ui, |ui, viewport| {
                        ui.set_min_size(Vec2::new(bit_panel_width, bit_content_height));

                        let viewport_start_row =
                            (viewport.min.y / bit_row_height).floor().max(0.0) as usize;
                        let viewport_end_row = (viewport.max.y / bit_row_height).ceil() as usize;
                        let viewport_start_col =
                            (viewport.min.x / self.bit_size).floor().max(0.0) as usize;
                        let viewport_end_col =
                            (viewport.max.x / self.bit_size).ceil().max(0.0) as usize;
                        let cache_start_row = viewport_start_row.saturating_sub(bit_overscan_rows);
                        let cache_end_row = (viewport_end_row + bit_overscan_rows).min(total_rows);
                        let cache_row_count = cache_end_row.saturating_sub(cache_start_row);
                        let cache_start_col = viewport_start_col.saturating_sub(bit_overscan_cols);
                        let cache_end_col =
                            (viewport_end_col + bit_overscan_cols).min(self.row_width_bits);
                        let cache_col_count = cache_end_col.saturating_sub(cache_start_col);

                        if let Some(texture_id) = self.ensure_bit_texture(
                            ui.ctx(),
                            layout.as_ref(),
                            cache_start_row,
                            cache_row_count,
                            cache_start_col,
                            cache_col_count,
                        ) {
                            let cache_rect = Rect::from_min_size(
                                egui::pos2(
                                    ui.max_rect().left() + cache_start_col as f32 * self.bit_size,
                                    ui.max_rect().top() + cache_start_row as f32 * bit_row_height,
                                ),
                                Vec2::new(
                                    cache_col_count as f32 * self.bit_size,
                                    cache_row_count as f32 * bit_row_height,
                                ),
                            );
                            ui.painter().image(
                                texture_id,
                                cache_rect,
                                Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                                Color32::WHITE,
                            );
                            paint_bit_grid_lines(
                                ui,
                                cache_rect,
                                cache_start_col,
                                cache_col_count,
                                cache_row_count,
                                self.bit_size,
                                bit_row_height,
                            );
                        }
                    });
                    observed_bit_scroll_row =
                        (output.state.offset.y / bit_row_height).floor().max(0.0) as usize;
                });
        }

        self.current_bit_scroll_row = observed_bit_scroll_row.min(total_rows.saturating_sub(1));
        self.current_text_scroll_row = observed_text_scroll_row.min(total_rows.saturating_sub(1));
    }

    fn ensure_bit_texture(
        &mut self,
        context: &Context,
        layout: &crate::viewer::RowLayout,
        start_row: usize,
        row_count: usize,
        start_col: usize,
        col_count: usize,
    ) -> Option<egui::TextureId> {
        if row_count == 0 || col_count == 0 || self.row_width_bits == 0 {
            return None;
        }

        let view = self.derived_view.as_ref()?;

        let key = BitTextureKey {
            view_revision: self.derived_view_revision,
            row_width_bits: self.row_width_bits,
            start_row,
            row_count,
            start_col,
            col_count,
        };

        if self.bit_texture_key != Some(key) {
            let bit_values =
                build_bit_window(view, layout, start_row, row_count, start_col, col_count);
            let pixels = bit_values
                .into_iter()
                .map(|bit| {
                    if bit == 1 {
                        BIT_ONE_COLOR
                    } else if bit == BIT_VALUE_NO_DATA {
                        Color32::TRANSPARENT
                    } else {
                        BIT_ZERO_COLOR
                    }
                })
                .collect::<Vec<_>>();
            let image = egui::ColorImage::new([col_count, row_count], pixels);

            if let Some(texture) = &mut self.bit_texture {
                texture.set(image, egui::TextureOptions::NEAREST);
            } else {
                self.bit_texture = Some(context.load_texture(
                    "bit-grid-viewport",
                    image,
                    egui::TextureOptions::NEAREST,
                ));
            }

            self.bit_texture_key = Some(key);
        }

        self.bit_texture.as_ref().map(TextureHandle::id)
    }

    fn start_file_dialog(&mut self) {
        if self.file_dialog_pending {
            return;
        }

        let (sender, receiver) = mpsc::channel();
        self.file_dialog_rx = Some(receiver);
        self.file_dialog_pending = true;
        self.last_error = None;

        thread::spawn(move || {
            let selection = FileDialog::new().pick_file();
            let _ = sender.send(selection);
        });
    }

    fn poll_file_dialog(&mut self) {
        let Some(receiver) = &self.file_dialog_rx else {
            return;
        };

        match receiver.try_recv() {
            Ok(Some(path)) => {
                self.file_dialog_rx = None;
                self.file_dialog_pending = false;
                self.load_document(path);
            }
            Ok(None) => {
                self.file_dialog_rx = None;
                self.file_dialog_pending = false;
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                self.file_dialog_rx = None;
                self.file_dialog_pending = false;
                self.last_error =
                    Some("File chooser failed before returning a selection.".to_owned());
            }
        }
    }

    fn poll_rebuild(&mut self) {
        let Some(receiver) = &self.rebuild_rx else {
            return;
        };

        match receiver.try_recv() {
            Ok(build_result) => {
                if build_result.request_id != self.rebuild_request_id {
                    return;
                }

                self.rebuild_rx = None;
                self.rebuild_pending = false;
                match build_result.result {
                    Ok(view) => {
                        self.derived_view = Some(view);
                        self.derived_view_revision = self.derived_view_revision.saturating_add(1);
                        self.invalidate_render_caches();
                        self.last_error = None;
                    }
                    Err(error) => {
                        self.derived_view = None;
                        self.invalidate_render_caches();
                        self.last_error = Some(error);
                    }
                }
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                self.rebuild_rx = None;
                self.rebuild_pending = false;
                self.last_error = Some("Filtered view worker stopped unexpectedly.".to_owned());
            }
        }
    }

    fn schedule_rebuild(&mut self) {
        let Some(document) = &self.document else {
            self.derived_view = None;
            return;
        };

        let path = document.path().to_path_buf();
        let pipeline = self.pipeline.clone();
        let request_id = self.rebuild_request_id.saturating_add(1);
        let (sender, receiver) = mpsc::channel();

        self.rebuild_request_id = request_id;
        self.rebuild_rx = Some(receiver);
        self.rebuild_pending = true;
        self.derived_view = None;
        self.invalidate_render_caches();
        self.pending_bit_scroll_to_row = Some(0);
        self.pending_text_scroll_to_row = Some(0);
        self.current_bit_scroll_row = 0;
        self.current_text_scroll_row = 0;
        self.last_error = None;

        thread::spawn(move || {
            let result = BinaryDocument::open(&path)
                .and_then(|document| build_derived_view(document.as_bytes(), &pipeline));
            let _ = sender.send(DerivedBuildResult { request_id, result });
        });
    }

    fn handle_file_drop(&mut self, context: &Context) {
        let dropped_path = context.input(|input| {
            input
                .raw
                .dropped_files
                .iter()
                .find_map(|file| file.path.clone())
        });

        if let Some(path) = dropped_path {
            self.load_document(path);
        }
    }

    fn open_path_input(&mut self) {
        let trimmed = self.path_input.trim();
        if trimmed.is_empty() {
            self.last_error = Some("Enter a file path first.".to_owned());
            return;
        }

        self.load_document(PathBuf::from(trimmed));
    }

    fn load_document(&mut self, path: PathBuf) {
        match BinaryDocument::open(path) {
            Ok(document) => {
                self.path_input = document.path().display().to_string();
                self.document = Some(document);
                self.schedule_rebuild();
            }
            Err(error) => {
                self.last_error = Some(error);
            }
        }
    }

    fn jump_to_byte(&mut self) {
        let parsed = self.jump_byte_input.trim().parse::<usize>();
        match parsed {
            Ok(byte_offset) => self.jump_to_bit_offset(byte_offset.saturating_mul(8)),
            Err(_) => {
                self.last_error = Some("Byte offset must be an integer.".to_owned());
            }
        }
    }

    fn jump_to_bit(&mut self) {
        let parsed = self.jump_bit_input.trim().parse::<usize>();
        match parsed {
            Ok(bit_offset) => self.jump_to_bit_offset(bit_offset),
            Err(_) => {
                self.last_error = Some("Bit offset must be an integer.".to_owned());
            }
        }
    }

    fn jump_to_bit_offset(&mut self, bit_offset: usize) {
        let Some(total_bits) = self.derived_view.as_ref().map(DerivedView::total_bits) else {
            self.last_error = Some("Wait for the filtered view to finish building.".to_owned());
            return;
        };

        if total_bits == 0 {
            self.last_error = Some("There are no bits to jump to in the current view.".to_owned());
            return;
        }

        let Some(layout) = self.ensure_row_layout() else {
            self.last_error = Some("Wait for the filtered view to finish building.".to_owned());
            return;
        };
        let Some(view) = self.derived_view.as_ref() else {
            self.last_error = Some("Wait for the filtered view to finish building.".to_owned());
            return;
        };
        let clamped = bit_offset.min(total_bits.saturating_sub(1));
        let row = bit_offset_to_row(view, layout.as_ref(), clamped);
        self.pending_bit_scroll_to_row = Some(row);
        self.pending_text_scroll_to_row = Some(row);
        self.last_error = None;
    }

    fn handle_keyboard_shortcuts(&mut self, context: &Context) {
        if context.wants_keyboard_input() {
            return;
        }

        context.input(|input| {
            if input.key_pressed(Key::H) {
                self.show_text_pane = !self.show_text_pane;
            }

            if input.key_pressed(Key::I) {
                self.show_shortcuts = !self.show_shortcuts;
            }

            if input.key_pressed(Key::OpenBracket) {
                self.decrease_row_width();
            }
            if input.key_pressed(Key::CloseBracket) {
                self.increase_row_width();
            }
            if input.key_pressed(Key::Minus) {
                self.decrease_bit_size();
            }
            if input.key_pressed(Key::Equals) || input.key_pressed(Key::Plus) {
                self.increase_bit_size();
            }
        });

        self.handle_keyboard_navigation(context);
    }

    fn handle_keyboard_navigation(&mut self, context: &Context) {
        if self.derived_view.is_none() {
            return;
        }

        let Some(layout) = self.ensure_row_layout() else {
            return;
        };
        let total_rows = layout.total_rows();
        if total_rows == 0 {
            return;
        }

        let step = context.input(|input| {
            if input.key_pressed(Key::ArrowDown) {
                Some(1isize)
            } else if input.key_pressed(Key::ArrowUp) {
                Some(-1)
            } else if input.key_pressed(Key::PageDown) {
                Some(20)
            } else if input.key_pressed(Key::PageUp) {
                Some(-20)
            } else if input.key_pressed(Key::Home) {
                self.pending_bit_scroll_to_row = Some(0);
                self.pending_text_scroll_to_row = Some(0);
                None
            } else if input.key_pressed(Key::End) {
                let last_row = total_rows.saturating_sub(1);
                self.pending_bit_scroll_to_row = Some(last_row);
                self.pending_text_scroll_to_row = Some(last_row);
                None
            } else {
                None
            }
        });

        let Some(step) = step else {
            return;
        };

        let current_row = self.current_bit_scroll_row;
        let next_row = if step.is_negative() {
            current_row.saturating_sub(step.unsigned_abs())
        } else {
            (current_row + step as usize).min(total_rows.saturating_sub(1))
        };
        self.pending_bit_scroll_to_row = Some(next_row);
        self.pending_text_scroll_to_row = Some(next_row);
    }

    fn advance_view_settings(&mut self, context: &Context) {
        let mut needs_repaint = false;

        if self.row_width_bits < self.target_row_width_bits {
            self.row_width_bits =
                (self.row_width_bits + ROW_WIDTH_STEP_BITS).min(self.target_row_width_bits);
            self.invalidate_render_caches();
            needs_repaint = true;
        } else if self.row_width_bits > self.target_row_width_bits {
            self.row_width_bits = self
                .row_width_bits
                .saturating_sub(ROW_WIDTH_STEP_BITS)
                .max(self.target_row_width_bits);
            self.invalidate_render_caches();
            needs_repaint = true;
        }

        if self.bit_size < self.target_bit_size {
            self.bit_size = (self.bit_size + BIT_SIZE_STEP).min(self.target_bit_size);
            needs_repaint = true;
        } else if self.bit_size > self.target_bit_size {
            self.bit_size = (self.bit_size - BIT_SIZE_STEP).max(self.target_bit_size);
            needs_repaint = true;
        }

        if needs_repaint {
            context.request_repaint();
        }
    }

    fn increase_row_width(&mut self) {
        self.target_row_width_bits =
            (self.target_row_width_bits + ROW_WIDTH_STEP_BITS).min(MAX_ROW_WIDTH_BITS);
    }

    fn decrease_row_width(&mut self) {
        self.target_row_width_bits = self
            .target_row_width_bits
            .saturating_sub(ROW_WIDTH_STEP_BITS)
            .max(MIN_ROW_WIDTH_BITS);
    }

    fn increase_bit_size(&mut self) {
        self.target_bit_size = (self.target_bit_size + BIT_SIZE_STEP).min(MAX_BIT_SIZE);
    }

    fn decrease_bit_size(&mut self) {
        self.target_bit_size = (self.target_bit_size - BIT_SIZE_STEP).max(MIN_BIT_SIZE);
    }
}

fn paint_bit_grid_lines(
    ui: &mut Ui,
    rect: Rect,
    start_col: usize,
    col_count: usize,
    row_count: usize,
    bit_size: f32,
    row_height: f32,
) {
    let painter = ui.painter();
    let top = rect.top();
    let left = rect.left();

    for bit_offset in 0..=col_count {
        let bit_index = start_col + bit_offset;
        let x = left + bit_offset as f32 * bit_size;
        let stroke = if bit_index > 0 && bit_index % 8 == 0 {
            Stroke::new(1.5, BYTE_DIVIDER_COLOR)
        } else {
            Stroke::new(0.5, BIT_BORDER_COLOR)
        };
        painter.line_segment([egui::pos2(x, top), egui::pos2(x, rect.bottom())], stroke);
    }

    for row_index in 0..=row_count {
        let y = top + row_index as f32 * row_height;
        painter.line_segment(
            [egui::pos2(left, y), egui::pos2(rect.right(), y)],
            Stroke::new(0.5, BIT_BORDER_COLOR),
        );
    }
}

fn paint_single_text_row(ui: &mut Ui, text: &str, row_height: f32, width: f32, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, row_height), egui::Sense::hover());
    ui.painter().text(
        egui::pos2(rect.left() + TEXT_CELL_PADDING_X, rect.center().y),
        egui::Align2::LEFT_CENTER,
        text,
        FontId::new(13.0, FontFamily::Monospace),
        color,
    );
}
