use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;

use eframe::egui::{
    self, Align, Color32, Context, FontFamily, FontId, Key, Layout, Rect, RichText, ScrollArea,
    Stroke, TextureHandle, Ui, Vec2,
};
use rfd::FileDialog;

use crate::document::BinaryDocument;
use crate::filters::{DerivedView, FilterPipeline, FilterStep, build_derived_view};
use crate::viewer::{bit_offset_to_row, build_bit_rows, build_row, build_row_layout};

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
const TEXT_OVERSCAN_ROWS: usize = 8;
const SCROLL_MULTIPLIER: Vec2 = Vec2::new(1.5, 2.5);
const BIT_ONE_COLOR: Color32 = Color32::from_rgb(32, 96, 246);
const BIT_ZERO_COLOR: Color32 = Color32::WHITE;
const BIT_BORDER_COLOR: Color32 = Color32::from_gray(200);
const BYTE_DIVIDER_COLOR: Color32 = Color32::from_rgb(220, 48, 48);
const PANEL_FILL: Color32 = Color32::from_rgb(248, 249, 252);
const HEX_COLUMN_MIN_WIDTH: f32 = 280.0;
const ASCII_COLUMN_MIN_WIDTH: f32 = 120.0;
const TEXT_PANEL_WIDTH: f32 = 540.0;
const POLL_INTERVAL: Duration = Duration::from_millis(50);

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
}

pub struct BitViewerApp {
    document: Option<BinaryDocument>,
    derived_view: Option<DerivedView>,
    pipeline: FilterPipeline,
    show_text_pane: bool,
    bit_texture: Option<TextureHandle>,
    bit_texture_key: Option<BitTextureKey>,
    derived_view_revision: u64,
    row_width_bits: usize,
    bit_size: f32,
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
            derived_view_revision: 0,
            row_width_bits: DEFAULT_ROW_WIDTH_BITS,
            bit_size: DEFAULT_BIT_SIZE,
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

impl eframe::App for BitViewerApp {
    fn update(&mut self, context: &Context, _frame: &mut eframe::Frame) {
        self.poll_file_dialog();
        self.poll_rebuild();
        self.handle_file_drop(context);
        self.handle_keyboard_shortcuts(context);

        if self.file_dialog_pending || self.rebuild_pending {
            context.request_repaint_after(POLL_INTERVAL);
        }

        egui::TopBottomPanel::top("toolbar")
            .frame(
                egui::Frame::new()
                    .fill(PANEL_FILL)
                    .stroke(Stroke::new(1.0, Color32::from_gray(220)))
                    .inner_margin(12.0),
            )
            .show(context, |ui| {
                self.show_toolbar(ui);
            });

        egui::TopBottomPanel::bottom("status_bar")
            .frame(
                egui::Frame::new()
                    .fill(PANEL_FILL)
                    .stroke(Stroke::new(1.0, Color32::from_gray(220)))
                    .inner_margin(8.0),
            )
            .show(context, |ui| {
                self.show_status(ui);
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(Color32::from_rgb(252, 252, 254)))
            .show(context, |ui| {
                if self.document.is_some() {
                    self.show_viewer(ui);
                } else {
                    self.show_empty_state(ui);
                }
            });

        self.show_shortcuts_window(context);
    }
}

impl BitViewerApp {
    fn show_toolbar(&mut self, ui: &mut Ui) {
        let mut rebuild_requested = false;

        ui.horizontal_wrapped(|ui| {
            if ui.button("Open file").clicked() {
                self.start_file_dialog();
            }

            if ui.button("Info").clicked() {
                self.show_shortcuts = true;
            }

            if self.file_dialog_pending {
                ui.label(RichText::new("opening chooser...").small().italics());
            }

            if self.rebuild_pending {
                ui.separator();
                ui.spinner();
                ui.label(
                    RichText::new("rebuilding filtered view...")
                        .small()
                        .italics(),
                );
            }

            ui.separator();

            ui.label("Row width");
            ui.add(
                egui::DragValue::new(&mut self.row_width_bits)
                    .range(MIN_ROW_WIDTH_BITS..=MAX_ROW_WIDTH_BITS)
                    .speed(1.0),
            );
            self.row_width_bits = self.row_width_bits.max(MIN_ROW_WIDTH_BITS);

            ui.label("Bit size");
            ui.add(
                egui::Slider::new(&mut self.bit_size, MIN_BIT_SIZE..=MAX_BIT_SIZE)
                    .clamping(egui::SliderClamping::Always)
                    .step_by(1.0),
            );

            ui.separator();
            ui.checkbox(&mut self.show_text_pane, "Show hex/ascii");
        });

        ui.horizontal_wrapped(|ui| {
            ui.label("Path");
            let path_response = ui.text_edit_singleline(&mut self.path_input);
            if path_response.lost_focus() && ui.input(|input| input.key_pressed(Key::Enter)) {
                self.open_path_input();
            }
            if ui.button("Open path").clicked() {
                self.open_path_input();
            }

            ui.separator();

            ui.label("Jump to byte");
            let byte_response = ui.text_edit_singleline(&mut self.jump_byte_input);
            if byte_response.lost_focus() && ui.input(|input| input.key_pressed(Key::Enter)) {
                self.jump_to_byte();
            }
            if ui.button("Jump byte").clicked() {
                self.jump_to_byte();
            }

            ui.separator();

            ui.label("Jump to bit");
            let bit_response = ui.text_edit_singleline(&mut self.jump_bit_input);
            if bit_response.lost_focus() && ui.input(|input| input.key_pressed(Key::Enter)) {
                self.jump_to_bit();
            }
            if ui.button("Jump bit").clicked() {
                self.jump_to_bit();
            }

            if let Some(error) = &self.last_error {
                ui.separator();
                ui.colored_label(Color32::from_rgb(190, 44, 44), error);
            }
        });

        ui.separator();
        rebuild_requested |= self.show_filter_editor(ui);

        if rebuild_requested {
            self.schedule_rebuild();
        }
    }

    fn show_filter_editor(&mut self, ui: &mut Ui) -> bool {
        let mut changed = false;
        let mut move_up = None;
        let mut move_down = None;
        let mut delete = None;

        ui.label(RichText::new("Filter pipeline").strong());
        ui.small("Filters run top to bottom. Group filters require a sync/group step earlier in the stack.");

        if self.pipeline.is_empty() {
            ui.label(
                RichText::new("No filters. The whole file is shown as one continuous stream.")
                    .small(),
            );
        }

        for index in 0..self.pipeline.steps.len() {
            ui.horizontal_wrapped(|ui| {
                ui.monospace(format!("{:02}.", index + 1));
                if ui.small_button("Up").clicked() && index > 0 {
                    move_up = Some(index);
                }
                if ui.small_button("Down").clicked() && index + 1 < self.pipeline.steps.len() {
                    move_down = Some(index);
                }
                if ui.small_button("Delete").clicked() {
                    delete = Some(index);
                }

                let step = &mut self.pipeline.steps[index];
                ui.label(step.label());

                match step {
                    FilterStep::SyncOnPreamble { bits } => {
                        ui.label("bits");
                        if ui.text_edit_singleline(bits).changed() {
                            changed = true;
                        }
                    }
                    FilterStep::ReverseBitsPerByte | FilterStep::InvertBits => {}
                    FilterStep::XorMask { mask } => {
                        ui.label("mask");
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
                    }
                    FilterStep::KeepGroupsLongerThanBytes { min_bytes } => {
                        ui.label(">");
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
                        ui.label("bytes");
                    }
                    FilterStep::SelectBitRangeFromGroup {
                        start_bit,
                        length_bits,
                    } => {
                        ui.label("start");
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
                        ui.label("length");
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
                        ui.label("bits");
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

        ui.horizontal_wrapped(|ui| {
            if ui.button("+ Sync").clicked() {
                self.pipeline.steps.push(FilterStep::SyncOnPreamble {
                    bits: "1010".to_owned(),
                });
                changed = true;
            }
            if ui.button("+ Reverse bytes").clicked() {
                self.pipeline.steps.push(FilterStep::ReverseBitsPerByte);
                changed = true;
            }
            if ui.button("+ Invert").clicked() {
                self.pipeline.steps.push(FilterStep::InvertBits);
                changed = true;
            }
            if ui.button("+ XOR").clicked() {
                self.pipeline.steps.push(FilterStep::XorMask { mask: 0xFF });
                changed = true;
            }
            if ui.button("+ Keep > bytes").clicked() {
                self.pipeline
                    .steps
                    .push(FilterStep::KeepGroupsLongerThanBytes { min_bytes: 6 });
                changed = true;
            }
            if ui.button("+ Select bits").clicked() {
                self.pipeline
                    .steps
                    .push(FilterStep::SelectBitRangeFromGroup {
                        start_bit: 0,
                        length_bits: 48,
                    });
                changed = true;
            }
            if ui.button("Clear pipeline").clicked() {
                self.pipeline.steps.clear();
                changed = true;
            }
        });

        changed
    }

    fn show_status(&self, ui: &mut Ui) {
        ui.horizontal_wrapped(|ui| {
            if let Some(document) = &self.document {
                ui.label(
                    RichText::new(format!(
                        "{} | source {} bytes | {} bits",
                        document.file_name(),
                        document.len_bytes(),
                        document.len_bits()
                    ))
                    .monospace(),
                );

                if let Some(view) = &self.derived_view {
                    ui.separator();
                    ui.label(
                        RichText::new(format!(
                            "derived {} groups | {} bits | {} bytes rounded",
                            view.group_count(),
                            view.total_bits(),
                            view.total_bytes_rounded_up()
                        ))
                        .monospace(),
                    );
                }

                if self.rebuild_pending {
                    ui.separator();
                    ui.spinner();
                    ui.label(RichText::new("processing filters").monospace());
                }

                if !self.pipeline.is_empty() {
                    ui.separator();
                    ui.label(
                        RichText::new(format!("{} filter(s) active", self.pipeline.steps.len()))
                            .monospace(),
                    );
                }

                ui.separator();
                ui.label(
                    RichText::new(document.path().display().to_string())
                        .monospace()
                        .small(),
                );
            } else {
                ui.label("Open a file to start exploring bits, hex, and ASCII.");
            }
        });
    }

    fn show_empty_state(&mut self, ui: &mut Ui) {
        ui.with_layout(Layout::top_down_justified(Align::Center), |ui| {
            ui.add_space(96.0);
            ui.heading("Bit Viewer Desktop");
            ui.label("Native Rust viewer for large binary files.");
            ui.add_space(12.0);
            if ui.button("Open file").clicked() {
                self.start_file_dialog();
            }
            ui.add_space(12.0);
            ui.label(
                "If the native file chooser stalls, paste a full path above or drag a file into the window.",
            );
        });
    }

    fn show_shortcuts_window(&mut self, context: &Context) {
        if !self.show_shortcuts {
            return;
        }

        egui::Window::new("Shortcuts")
            .open(&mut self.show_shortcuts)
            .resizable(false)
            .collapsible(false)
            .show(context, |ui| {
                ui.label(RichText::new("Viewer").strong());
                ui.monospace("[ / ]     Decrease / increase row width");
                ui.monospace("- / =     Decrease / increase bit size");
                ui.monospace("I         Toggle this shortcuts window");
                ui.separator();
                ui.label(RichText::new("Navigation").strong());
                ui.monospace("Arrow Up / Down   Scroll by 1 row");
                ui.monospace("Page Up / Down    Scroll by 20 rows");
                ui.monospace("Home / End        Jump to start / end");
            });
    }

    fn show_viewer(&mut self, ui: &mut Ui) {
        if self.derived_view.is_none() {
            ui.centered_and_justified(|ui| {
                if self.rebuild_pending {
                    ui.spinner();
                    ui.label("Building filtered view...");
                } else {
                    ui.label("No derived view is available.");
                }
            });
            return;
        }

        let layout = {
            let view = self
                .derived_view
                .as_ref()
                .expect("derived view should exist after early return");
            build_row_layout(view, self.row_width_bits)
        };
        let total_rows = layout.total_rows();
        if total_rows == 0 {
            ui.centered_and_justified(|ui| {
                ui.label("No rows remain after applying the current filters.");
            });
            return;
        }

        let pending_bit_scroll_to_row = self.pending_bit_scroll_to_row.take();
        let pending_text_scroll_to_row = self.pending_text_scroll_to_row.take();
        let bit_row_height = self.bit_size;
        let text_row_height = TEXT_ROW_HEIGHT;
        let bytes_per_row = self.row_width_bits.div_ceil(8);
        let hex_width = HEX_COLUMN_MIN_WIDTH.max(bytes_per_row as f32 * 21.0);
        let ascii_width = ASCII_COLUMN_MIN_WIDTH.max(bytes_per_row as f32 * 10.0);
        let bit_panel_width = self.row_width_bits as f32 * self.bit_size;
        let bit_content_height = total_rows as f32 * bit_row_height;
        let available_height = ui.available_height();
        let mut observed_bit_scroll_row = self.current_bit_scroll_row;
        let mut observed_text_scroll_row = self.current_text_scroll_row;

        ui.horizontal(|ui| {
            let bit_area_width = if self.show_text_pane {
                ui.available_width() - TEXT_PANEL_WIDTH
            } else {
                ui.available_width()
            };
            ui.allocate_ui_with_layout(
                Vec2::new(bit_area_width, available_height),
                Layout::top_down(Align::Min),
                |ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);

                    let mut scroll_area = ScrollArea::both()
                        .id_salt("native-bit-scroll")
                        .auto_shrink([false, false])
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
                        let cache_start_row = viewport_start_row.saturating_sub(BIT_OVERSCAN_ROWS);
                        let cache_end_row = (viewport_end_row + BIT_OVERSCAN_ROWS).min(total_rows);
                        let cache_row_count = cache_end_row.saturating_sub(cache_start_row);

                        if let Some(texture_id) = self.ensure_bit_texture(
                            ui.ctx(),
                            &layout,
                            cache_start_row,
                            cache_row_count,
                        ) {
                            let cache_rect = Rect::from_min_size(
                                egui::pos2(
                                    ui.max_rect().left(),
                                    ui.max_rect().top() + cache_start_row as f32 * bit_row_height,
                                ),
                                Vec2::new(bit_panel_width, cache_row_count as f32 * bit_row_height),
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
                                self.row_width_bits,
                                cache_row_count,
                                self.bit_size,
                                bit_row_height,
                            );
                        }
                    });
                    observed_bit_scroll_row =
                        (output.state.offset.y / bit_row_height).floor().max(0.0) as usize;
                },
            );

            if self.show_text_pane {
                ui.separator();

                ui.allocate_ui_with_layout(
                    Vec2::new(TEXT_PANEL_WIDTH, available_height),
                    Layout::top_down(Align::Min),
                    |ui| {
                        let mut scroll_area = ScrollArea::both()
                            .id_salt("native-text-scroll")
                            .auto_shrink([false, false])
                            .wheel_scroll_multiplier(SCROLL_MULTIPLIER);

                        if let Some(target_row) = pending_text_scroll_to_row {
                            scroll_area = scroll_area
                                .vertical_scroll_offset(target_row as f32 * text_row_height);
                        }

                        let output = scroll_area.show_rows(
                            ui,
                            text_row_height,
                            total_rows,
                            |ui, row_range| {
                                let start = row_range.start.saturating_sub(TEXT_OVERSCAN_ROWS);
                                let end = (row_range.end + TEXT_OVERSCAN_ROWS).min(total_rows);

                                let Some(view) = self.derived_view.as_ref() else {
                                    return;
                                };
                                for row_index in start..end {
                                    let row = build_row(view, &layout, row_index);
                                    paint_text_row(
                                        ui,
                                        &row.hex,
                                        &row.ascii,
                                        text_row_height,
                                        hex_width,
                                        ascii_width,
                                    );
                                }
                            },
                        );
                        observed_text_scroll_row =
                            (output.state.offset.y / text_row_height).floor().max(0.0) as usize;
                    },
                );
            }
        });

        self.current_bit_scroll_row = observed_bit_scroll_row.min(total_rows.saturating_sub(1));
        self.current_text_scroll_row = observed_text_scroll_row.min(total_rows.saturating_sub(1));
    }

    fn ensure_bit_texture(
        &mut self,
        context: &Context,
        layout: &crate::viewer::RowLayout,
        start_row: usize,
        row_count: usize,
    ) -> Option<egui::TextureId> {
        if row_count == 0 || self.row_width_bits == 0 {
            return None;
        }

        let view = self.derived_view.as_ref()?;

        let key = BitTextureKey {
            view_revision: self.derived_view_revision,
            row_width_bits: self.row_width_bits,
            start_row,
            row_count,
        };

        if self.bit_texture_key != Some(key) {
            let bit_values = build_bit_rows(view, layout, start_row, row_count);
            let pixels = bit_values
                .into_iter()
                .map(|bit| {
                    if bit == 1 {
                        BIT_ONE_COLOR
                    } else {
                        BIT_ZERO_COLOR
                    }
                })
                .collect::<Vec<_>>();
            let image = egui::ColorImage::new([self.row_width_bits, row_count], pixels);

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
                        self.bit_texture = None;
                        self.bit_texture_key = None;
                        self.last_error = None;
                    }
                    Err(error) => {
                        self.derived_view = None;
                        self.bit_texture = None;
                        self.bit_texture_key = None;
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
        self.bit_texture = None;
        self.bit_texture_key = None;
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
        let Some(view) = &self.derived_view else {
            self.last_error = Some("Wait for the filtered view to finish building.".to_owned());
            return;
        };

        if view.total_bits() == 0 {
            self.last_error = Some("There are no bits to jump to in the current view.".to_owned());
            return;
        }

        let layout = build_row_layout(view, self.row_width_bits);
        let clamped = bit_offset.min(view.total_bits().saturating_sub(1));
        let row = bit_offset_to_row(view, &layout, clamped);
        self.pending_bit_scroll_to_row = Some(row);
        self.pending_text_scroll_to_row = Some(row);
        self.last_error = None;
    }

    fn handle_keyboard_shortcuts(&mut self, context: &Context) {
        if context.wants_keyboard_input() {
            return;
        }

        context.input(|input| {
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
        let Some(view) = &self.derived_view else {
            return;
        };

        let layout = build_row_layout(view, self.row_width_bits);
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

    fn increase_row_width(&mut self) {
        self.row_width_bits = (self.row_width_bits + ROW_WIDTH_STEP_BITS).min(MAX_ROW_WIDTH_BITS);
    }

    fn decrease_row_width(&mut self) {
        self.row_width_bits = self
            .row_width_bits
            .saturating_sub(ROW_WIDTH_STEP_BITS)
            .max(MIN_ROW_WIDTH_BITS);
    }

    fn increase_bit_size(&mut self) {
        self.bit_size = (self.bit_size + BIT_SIZE_STEP).min(MAX_BIT_SIZE);
    }

    fn decrease_bit_size(&mut self) {
        self.bit_size = (self.bit_size - BIT_SIZE_STEP).max(MIN_BIT_SIZE);
    }
}

fn paint_bit_grid_lines(
    ui: &mut Ui,
    rect: Rect,
    row_width_bits: usize,
    row_count: usize,
    bit_size: f32,
    row_height: f32,
) {
    let painter = ui.painter();
    let top = rect.top();
    let left = rect.left();

    for bit_index in 0..=row_width_bits {
        let x = left + bit_index as f32 * bit_size;
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

fn paint_text_row(
    ui: &mut Ui,
    hex: &str,
    ascii: &str,
    row_height: f32,
    hex_width: f32,
    ascii_width: f32,
) {
    ui.horizontal(|ui| {
        let (hex_rect, _) =
            ui.allocate_exact_size(Vec2::new(hex_width, row_height), egui::Sense::hover());
        ui.painter().text(
            hex_rect.left_center(),
            egui::Align2::LEFT_CENTER,
            hex,
            FontId::new(13.0, FontFamily::Monospace),
            Color32::from_gray(40),
        );

        let (ascii_rect, _) =
            ui.allocate_exact_size(Vec2::new(ascii_width, row_height), egui::Sense::hover());
        ui.painter().text(
            ascii_rect.left_center(),
            egui::Align2::LEFT_CENTER,
            ascii,
            FontId::new(13.0, FontFamily::Monospace),
            Color32::from_gray(64),
        );
    });
}
