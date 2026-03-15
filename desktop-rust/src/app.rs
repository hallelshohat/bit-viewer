use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;

use eframe::egui::{
    self, Align, Color32, Context, CornerRadius, FontFamily, FontId, Key, Layout, PointerButton,
    Rect, RichText, ScrollArea, Stroke, TextureHandle, Ui, Vec2,
};
use egui::containers::menu::{MenuButton, MenuConfig};
use rfd::FileDialog;

use crate::autocorrelation::{
    AutoCorrelationResult, analyze_width_autocorrelation_limited_with_progress,
    autocorrelation_width_limit_limited,
};
use crate::document::BinaryDocument;
use crate::export::{
    ExportFormat, LinkTypeOption, PcapExportOptions, WAV_CODEC_PRESETS, WavExportOptions,
    default_export_file_name, export_flattened_bits, export_pcap, export_wav, known_link_types,
};
use crate::filters::{DerivedView, FilterPipeline, FilterStep};
use crate::viewer::{
    BIT_VALUE_NO_DATA, RowData, RowLayout, bit_offset_to_row, build_bit_window, build_row,
    build_row_layout,
};

const DEFAULT_ROW_WIDTH_BITS: usize = 128;
const MIN_ROW_WIDTH_BITS: usize = 1;
const MAX_ROW_WIDTH_BITS: usize = usize::MAX;
const DEFAULT_AUTOCORRELATION_MAX_WIDTH_BITS: usize = 512;
const MIN_AUTOCORRELATION_MAX_WIDTH_BITS: usize = 1;
const MAX_AUTOCORRELATION_MAX_WIDTH_BITS: usize = 8_192;
const DEFAULT_AUTOCORRELATION_SAMPLE_BYTES: usize = 1_048_576;
const MIN_AUTOCORRELATION_SAMPLE_BYTES: usize = 1;
const MAX_AUTOCORRELATION_SAMPLE_BYTES: usize = 64 * 1_048_576;
const AUTOCORRELATION_WINDOW_WIDTH: f32 = 640.0;
const AUTOCORRELATION_WINDOW_DEFAULT_HEIGHT: f32 = 420.0;
const AUTOCORRELATION_WINDOW_MIN_HEIGHT: f32 = 320.0;
const AUTOCORRELATION_GRAPH_MIN_WIDTH: f32 = 280.0;
const AUTOCORRELATION_GRAPH_MAX_WIDTH: f32 = 520.0;
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
const BIT_ONE_COLOR: Color32 = Color32::from_rgb(49, 121, 255);
const BIT_ZERO_COLOR: Color32 = Color32::from_rgb(222, 231, 245);
const BIT_BORDER_COLOR: Color32 = Color32::from_rgb(92, 109, 138);
const BYTE_DIVIDER_COLOR: Color32 = Color32::from_rgb(255, 183, 76);
const APP_BG: Color32 = Color32::from_rgb(8, 14, 24);
const SURFACE_BG: Color32 = Color32::from_rgb(22, 31, 46);
const SURFACE_ALT_BG: Color32 = Color32::from_rgb(29, 40, 59);
const SURFACE_SUBTLE_BG: Color32 = Color32::from_rgb(36, 49, 71);
const TOP_BAR_MENU_BG: Color32 = Color32::from_rgb(24, 39, 66);
const TOP_BAR_MENU_ALT_BG: Color32 = Color32::from_rgb(31, 50, 84);
const BORDER_COLOR: Color32 = Color32::from_rgb(72, 92, 126);
const ACCENT_COLOR: Color32 = Color32::from_rgb(118, 203, 255);
const ACCENT_SOFT: Color32 = Color32::from_rgb(44, 96, 148);
const TEXT_PRIMARY: Color32 = Color32::from_rgb(245, 248, 255);
const TEXT_MUTED: Color32 = Color32::from_rgb(182, 194, 217);
const ERROR_COLOR: Color32 = Color32::from_rgb(255, 119, 119);
const SUCCESS_BG: Color32 = Color32::from_rgb(19, 54, 42);
const SUCCESS_BORDER: Color32 = Color32::from_rgb(54, 122, 96);
const HEX_COLUMN_MIN_WIDTH: f32 = 260.0;
const ASCII_COLUMN_MIN_WIDTH: f32 = 150.0;
const ASCII_COLUMN_DEFAULT_WIDTH: f32 = 180.0;
const TEXT_FONT_SIZE: f32 = 13.0;
const TEXT_PANEL_MIN_WIDTH: f32 = 460.0;
const TEXT_PANEL_MAX_SHARE: f32 = 0.42;
const BIT_PANEL_MIN_WIDTH: f32 = 320.0;
const BIT_PANEL_DEFAULT_MAX_WIDTH: f32 = 760.0;
const POLL_INTERVAL: Duration = Duration::from_millis(50);
const TEXT_CELL_PADDING_X: f32 = 10.0;
const VIEWER_PANEL_GAP: f32 = 14.0;
const AUTOCORRELATION_GRAPH_HEIGHT: f32 = 168.0;
const WAV_SAMPLE_RATE_PRESETS: [u32; 14] = [
    8_000, 11_025, 12_000, 16_000, 22_050, 24_000, 32_000, 44_100, 48_000, 88_200, 96_000, 176_400,
    192_000, 384_000,
];
const MAX_WAV_CHANNELS: u16 = 16;

struct DerivedBuildResult {
    request_id: u64,
    result: Result<DerivedView, String>,
}

struct AutoCorrelationWorkerResult {
    request_id: u64,
    view_revision: u64,
    result: AutoCorrelationResult,
}

struct ExportSaveDialogResult {
    request_id: u64,
    path: Option<PathBuf>,
}

#[derive(Clone)]
struct PendingExportRequest {
    request_id: u64,
    format: ExportFormat,
    view: DerivedView,
    pcap: PcapExportOptions,
    wav: WavExportOptions,
}

struct ExportWorkerResult {
    request_id: u64,
    format: ExportFormat,
    path: PathBuf,
    result: Result<(), String>,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DrawGranularity {
    Bit,
    Byte,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DrawStrokeMode {
    Paint,
    Erase,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ActiveDrawStroke {
    mode: DrawStrokeMode,
    granularity: DrawGranularity,
    last_index: usize,
}

pub struct BitViewerApp {
    document: Option<BinaryDocument>,
    derived_view: Option<DerivedView>,
    pipeline: FilterPipeline,
    show_text_pane: bool,
    show_autocorrelation_panel: bool,
    bit_texture: Option<TextureHandle>,
    bit_texture_key: Option<BitTextureKey>,
    row_layout_cache: Option<CachedRowLayout>,
    text_row_cache: Option<CachedTextRows>,
    derived_view_revision: u64,
    row_width_bits: usize,
    target_row_width_bits: usize,
    row_width_input: String,
    bit_size: f32,
    target_bit_size: f32,
    autocorrelation_result: Option<AutoCorrelationResult>,
    autocorrelation_max_width_bits: usize,
    autocorrelation_max_width_input: String,
    autocorrelation_sample_bytes: usize,
    autocorrelation_sample_bytes_input: String,
    jump_bit_input: String,
    jump_byte_input: String,
    path_input: String,
    pending_bit_scroll_to_row: Option<usize>,
    pending_text_scroll_to_row: Option<usize>,
    current_bit_scroll_row: usize,
    current_text_scroll_row: usize,
    drawn_bit_columns: BTreeSet<usize>,
    active_draw_stroke: Option<ActiveDrawStroke>,
    file_dialog_rx: Option<Receiver<Option<PathBuf>>>,
    file_dialog_pending: bool,
    rebuild_rx: Option<Receiver<DerivedBuildResult>>,
    rebuild_pending: bool,
    rebuild_request_id: u64,
    autocorrelation_rx: Option<Receiver<AutoCorrelationWorkerResult>>,
    autocorrelation_pending: bool,
    autocorrelation_request_id: u64,
    autocorrelation_progress: Option<Arc<AtomicUsize>>,
    autocorrelation_progress_total: usize,
    show_export_window: bool,
    export_format: ExportFormat,
    link_type_options: Vec<LinkTypeOption>,
    selected_link_type: u32,
    pcap_link_search: String,
    pcap_use_custom_link_type: bool,
    pcap_custom_link_type: u32,
    pcap_timestamp_step_micros: u32,
    wav_options: WavExportOptions,
    export_save_dialog_rx: Option<Receiver<ExportSaveDialogResult>>,
    export_save_dialog_pending: bool,
    pending_export_request: Option<PendingExportRequest>,
    export_rx: Option<Receiver<ExportWorkerResult>>,
    export_pending: bool,
    export_request_id: u64,
    show_shortcuts: bool,
    last_export_message: Option<String>,
    last_error: Option<String>,
}

impl Default for BitViewerApp {
    fn default() -> Self {
        Self {
            document: None,
            derived_view: None,
            pipeline: FilterPipeline::default(),
            show_text_pane: true,
            show_autocorrelation_panel: false,
            bit_texture: None,
            bit_texture_key: None,
            row_layout_cache: None,
            text_row_cache: None,
            derived_view_revision: 0,
            row_width_bits: DEFAULT_ROW_WIDTH_BITS,
            target_row_width_bits: DEFAULT_ROW_WIDTH_BITS,
            row_width_input: DEFAULT_ROW_WIDTH_BITS.to_string(),
            bit_size: DEFAULT_BIT_SIZE,
            target_bit_size: DEFAULT_BIT_SIZE,
            autocorrelation_result: None,
            autocorrelation_max_width_bits: DEFAULT_AUTOCORRELATION_MAX_WIDTH_BITS,
            autocorrelation_max_width_input: DEFAULT_AUTOCORRELATION_MAX_WIDTH_BITS.to_string(),
            autocorrelation_sample_bytes: DEFAULT_AUTOCORRELATION_SAMPLE_BYTES,
            autocorrelation_sample_bytes_input: DEFAULT_AUTOCORRELATION_SAMPLE_BYTES.to_string(),
            jump_bit_input: String::new(),
            jump_byte_input: String::new(),
            path_input: String::new(),
            pending_bit_scroll_to_row: None,
            pending_text_scroll_to_row: None,
            current_bit_scroll_row: 0,
            current_text_scroll_row: 0,
            drawn_bit_columns: BTreeSet::new(),
            active_draw_stroke: None,
            file_dialog_rx: None,
            file_dialog_pending: false,
            rebuild_rx: None,
            rebuild_pending: false,
            rebuild_request_id: 0,
            autocorrelation_rx: None,
            autocorrelation_pending: false,
            autocorrelation_request_id: 0,
            autocorrelation_progress: None,
            autocorrelation_progress_total: 0,
            show_export_window: false,
            export_format: ExportFormat::FlattenedBits,
            link_type_options: known_link_types(),
            selected_link_type: 1,
            pcap_link_search: String::new(),
            pcap_use_custom_link_type: false,
            pcap_custom_link_type: 1,
            pcap_timestamp_step_micros: 1,
            wav_options: WavExportOptions::default(),
            export_save_dialog_rx: None,
            export_save_dialog_pending: false,
            pending_export_request: None,
            export_rx: None,
            export_pending: false,
            export_request_id: 0,
            show_shortcuts: false,
            last_export_message: None,
            last_error: None,
        }
    }
}

impl BitViewerApp {
    pub fn new(cc: &eframe::CreationContext<'_>, initial_path: Option<PathBuf>) -> Self {
        Self::configure_theme(&cc.egui_ctx);
        let mut app = Self::default();
        if let Some(path) = initial_path {
            app.load_document(path);
        }
        app
    }

    fn configure_theme(context: &Context) {
        let mut style = (*context.style()).clone();
        style.spacing.item_spacing = egui::vec2(12.0, 12.0);
        style.spacing.window_margin = egui::Margin::same(18);
        style.spacing.menu_margin = egui::Margin::same(14);
        style.spacing.button_padding = egui::vec2(14.0, 9.0);
        style.spacing.interact_size = egui::vec2(44.0, 34.0);
        style.spacing.slider_width = 170.0;
        style.spacing.text_edit_width = 240.0;
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
        visuals.extreme_bg_color = Color32::from_rgb(16, 23, 34);
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
        visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT_PRIMARY);
        visuals.widgets.inactive.bg_fill = SURFACE_ALT_BG;
        visuals.widgets.inactive.weak_bg_fill = SURFACE_ALT_BG;
        visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER_COLOR);
        visuals.widgets.inactive.corner_radius = CornerRadius::same(14);
        visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT_PRIMARY);
        visuals.widgets.hovered.bg_fill = Color32::from_rgb(42, 57, 84);
        visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(42, 57, 84);
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT_COLOR);
        visuals.widgets.hovered.corner_radius = CornerRadius::same(16);
        visuals.widgets.hovered.fg_stroke = Stroke::new(1.2, TEXT_PRIMARY);
        visuals.widgets.active.bg_fill = Color32::from_rgb(54, 74, 108);
        visuals.widgets.active.weak_bg_fill = Color32::from_rgb(54, 74, 108);
        visuals.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT_COLOR);
        visuals.widgets.active.corner_radius = CornerRadius::same(16);
        visuals.widgets.active.fg_stroke = Stroke::new(1.3, TEXT_PRIMARY);
        visuals.widgets.open.bg_fill = Color32::from_rgb(40, 54, 79);
        visuals.widgets.open.weak_bg_fill = Color32::from_rgb(40, 54, 79);
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
        self.poll_autocorrelation();
        self.poll_export_save_dialog();
        self.poll_export();
        self.handle_file_drop(context);
        self.finish_active_draw_stroke_on_release(context);
        self.handle_keyboard_shortcuts(context);
        self.advance_view_settings(context);

        if self.file_dialog_pending
            || self.rebuild_pending
            || self.autocorrelation_pending
            || self.export_save_dialog_pending
            || self.export_pending
        {
            context.request_repaint_after(POLL_INTERVAL);
        }

        egui::TopBottomPanel::top("top_bar")
            .frame(
                egui::Frame::new()
                    .fill(SURFACE_BG)
                    .stroke(Stroke::new(1.0, BORDER_COLOR))
                    .inner_margin(egui::Margin::symmetric(16, 12)),
            )
            .show(context, |ui| {
                self.show_top_bar(ui);
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
            .frame(
                egui::Frame::new()
                    .fill(APP_BG)
                    .inner_margin(egui::Margin::symmetric(16, 14)),
            )
            .show(context, |ui| {
                if self.document.is_some() {
                    self.show_main_content(ui);
                } else {
                    self.show_empty_state(ui);
                }
            });

        self.show_shortcuts_window(context);
        self.show_autocorrelation_window(context);
        self.show_export_window(context);
    }
}

impl BitViewerApp {
    fn invalidate_render_caches(&mut self) {
        self.bit_texture = None;
        self.bit_texture_key = None;
        self.row_layout_cache = None;
        self.text_row_cache = None;
    }

    fn clear_autocorrelation_state(&mut self) {
        self.autocorrelation_result = None;
        self.autocorrelation_rx = None;
        self.autocorrelation_pending = false;
        self.autocorrelation_progress = None;
        self.autocorrelation_progress_total = 0;
    }

    fn schedule_autocorrelation(&mut self) {
        let Some(view) = self.derived_view.clone() else {
            self.clear_autocorrelation_state();
            return;
        };

        let request_id = self.autocorrelation_request_id.saturating_add(1);
        let view_revision = self.derived_view_revision;
        let max_width_bits = self.autocorrelation_max_width_bits;
        let sample_bytes = self.autocorrelation_sample_bytes;
        let (sender, receiver) = mpsc::channel();
        let progress = Arc::new(AtomicUsize::new(0));
        let progress_total =
            autocorrelation_width_limit_limited(&view, max_width_bits, sample_bytes).max(1);

        self.autocorrelation_request_id = request_id;
        self.autocorrelation_rx = Some(receiver);
        self.autocorrelation_pending = true;
        self.autocorrelation_result = None;
        self.autocorrelation_progress = Some(Arc::clone(&progress));
        self.autocorrelation_progress_total = progress_total;

        thread::spawn(move || {
            let progress_for_worker = Arc::clone(&progress);
            let result = analyze_width_autocorrelation_limited_with_progress(
                &view,
                max_width_bits,
                sample_bytes,
                move |completed, _total| {
                    progress_for_worker.store(completed, Ordering::Relaxed);
                },
            );
            let _ = sender.send(AutoCorrelationWorkerResult {
                request_id,
                view_revision,
                result,
            });
        });
    }

    fn autocorrelation_progress(&self) -> Option<(usize, usize, f32)> {
        let total = self.autocorrelation_progress_total;
        if total == 0 {
            return None;
        }

        let completed = self
            .autocorrelation_progress
            .as_ref()
            .map(|progress| progress.load(Ordering::Relaxed).min(total))
            .unwrap_or(0);
        let fraction = completed as f32 / total as f32;
        Some((completed, total, fraction))
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

    fn clear_drawn_columns(&mut self) {
        self.drawn_bit_columns.clear();
        self.active_draw_stroke = None;
    }

    fn finish_active_draw_stroke_on_release(&mut self, context: &Context) {
        if self.active_draw_stroke.is_some()
            && !context.input(|input| input.pointer.secondary_down())
        {
            self.active_draw_stroke = None;
        }
    }

    fn trim_drawn_columns_to_row_width(&mut self) {
        self.drawn_bit_columns = self
            .drawn_bit_columns
            .iter()
            .copied()
            .filter(|bit_col| *bit_col < self.row_width_bits)
            .collect();

        if let Some(stroke) = self.active_draw_stroke
            && stroke.last_index >= self.granularity_limit(stroke.granularity)
        {
            self.active_draw_stroke = None;
        }
    }

    fn highlighted_bit_ranges(&self) -> Vec<(usize, usize)> {
        let mut ranges = Vec::new();
        let mut start = None;
        let mut previous = None;

        for bit_col in self.drawn_bit_columns.iter().copied() {
            match (start, previous) {
                (None, _) => {
                    start = Some(bit_col);
                    previous = Some(bit_col);
                }
                (Some(range_start), Some(last)) if bit_col == last + 1 => {
                    start = Some(range_start);
                    previous = Some(bit_col);
                }
                (Some(range_start), Some(last)) => {
                    ranges.push((range_start, last + 1));
                    start = Some(bit_col);
                    previous = Some(bit_col);
                }
                _ => {}
            }
        }

        if let (Some(range_start), Some(last)) = (start, previous) {
            ranges.push((range_start, last + 1));
        }

        ranges
    }

    fn highlighted_byte_ranges(&self) -> Vec<(usize, usize)> {
        let mut ranges = Vec::new();
        let mut start = None;
        let mut previous = None;
        let mut last_byte = None;

        for bit_col in self.drawn_bit_columns.iter().copied() {
            let byte_col = bit_col / 8;
            if last_byte == Some(byte_col) {
                continue;
            }
            last_byte = Some(byte_col);

            match (start, previous) {
                (None, _) => {
                    start = Some(byte_col);
                    previous = Some(byte_col);
                }
                (Some(range_start), Some(last)) if byte_col == last + 1 => {
                    start = Some(range_start);
                    previous = Some(byte_col);
                }
                (Some(range_start), Some(last)) => {
                    ranges.push((range_start, last + 1));
                    start = Some(byte_col);
                    previous = Some(byte_col);
                }
                _ => {}
            }
        }

        if let (Some(range_start), Some(last)) = (start, previous) {
            ranges.push((range_start, last + 1));
        }

        ranges
    }

    fn is_bit_drawn(&self, bit_col: usize) -> bool {
        self.drawn_bit_columns.contains(&bit_col)
    }

    fn is_byte_drawn(&self, byte_col: usize) -> bool {
        let start_bit = byte_col.saturating_mul(8);
        let end_bit = ((byte_col + 1).saturating_mul(8)).min(self.row_width_bits);
        self.drawn_bit_columns
            .range(start_bit..end_bit)
            .next()
            .is_some()
    }

    fn granularity_limit(&self, granularity: DrawGranularity) -> usize {
        match granularity {
            DrawGranularity::Bit => self.row_width_bits,
            DrawGranularity::Byte => self.row_width_bits.div_ceil(8),
        }
    }

    fn apply_draw_segment(
        &mut self,
        granularity: DrawGranularity,
        start_index: usize,
        end_index: usize,
        mode: DrawStrokeMode,
    ) {
        let (start, end) = if start_index <= end_index {
            (start_index, end_index)
        } else {
            (end_index, start_index)
        };

        match granularity {
            DrawGranularity::Bit => {
                for bit_col in start..=end {
                    match mode {
                        DrawStrokeMode::Paint => {
                            self.drawn_bit_columns.insert(bit_col);
                        }
                        DrawStrokeMode::Erase => {
                            self.drawn_bit_columns.remove(&bit_col);
                        }
                    }
                }
            }
            DrawGranularity::Byte => {
                for byte_col in start..=end {
                    let start_bit = byte_col.saturating_mul(8);
                    let end_bit = ((byte_col + 1).saturating_mul(8)).min(self.row_width_bits);
                    for bit_col in start_bit..end_bit {
                        match mode {
                            DrawStrokeMode::Paint => {
                                self.drawn_bit_columns.insert(bit_col);
                            }
                            DrawStrokeMode::Erase => {
                                self.drawn_bit_columns.remove(&bit_col);
                            }
                        }
                    }
                }
            }
        }
    }

    fn start_draw_stroke(&mut self, granularity: DrawGranularity, index: usize) {
        let mode = match granularity {
            DrawGranularity::Bit => {
                if self.is_bit_drawn(index) {
                    DrawStrokeMode::Erase
                } else {
                    DrawStrokeMode::Paint
                }
            }
            DrawGranularity::Byte => {
                if self.is_byte_drawn(index) {
                    DrawStrokeMode::Erase
                } else {
                    DrawStrokeMode::Paint
                }
            }
        };

        self.apply_draw_segment(granularity, index, index, mode);
        self.active_draw_stroke = Some(ActiveDrawStroke {
            mode,
            granularity,
            last_index: index,
        });
    }

    fn update_draw_stroke(&mut self, granularity: DrawGranularity, index: usize) {
        match self.active_draw_stroke {
            Some(mut stroke) if stroke.granularity == granularity => {
                self.apply_draw_segment(granularity, stroke.last_index, index, stroke.mode);
                stroke.last_index = index;
                self.active_draw_stroke = Some(stroke);
            }
            Some(stroke) => {
                self.apply_draw_segment(granularity, index, index, stroke.mode);
                self.active_draw_stroke = Some(ActiveDrawStroke {
                    mode: stroke.mode,
                    granularity,
                    last_index: index,
                });
            }
            None => {
                self.start_draw_stroke(granularity, index);
            }
        }
    }

    fn handle_draw_input(
        &mut self,
        ui: &Ui,
        rect: Rect,
        granularity: DrawGranularity,
        index: Option<usize>,
    ) {
        let (pointer_pos, secondary_pressed, secondary_down) = ui.ctx().input(|input| {
            (
                input.pointer.interact_pos(),
                input.pointer.secondary_pressed(),
                input.pointer.secondary_down(),
            )
        });

        if !secondary_down {
            return;
        }

        let Some(pointer_pos) = pointer_pos else {
            return;
        };
        if !rect.contains(pointer_pos) {
            return;
        }
        let Some(index) = index else {
            return;
        };
        if index >= self.granularity_limit(granularity) {
            return;
        }

        if secondary_pressed {
            self.start_draw_stroke(granularity, index);
            ui.ctx().request_repaint();
            return;
        }

        self.update_draw_stroke(granularity, index);
        ui.ctx().request_repaint();
    }

    fn show_top_bar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.scope(|ui| {
                ui.spacing_mut().button_padding = egui::vec2(14.0, 9.0);
                ui.spacing_mut().interact_size = egui::vec2(44.0, 34.0);

                let visuals = &mut ui.style_mut().visuals;
                visuals.override_text_color = Some(TEXT_PRIMARY);
                visuals.panel_fill = TOP_BAR_MENU_BG;
                visuals.window_fill = TOP_BAR_MENU_BG;
                visuals.faint_bg_color = TOP_BAR_MENU_ALT_BG;
                visuals.extreme_bg_color = TOP_BAR_MENU_ALT_BG;
                visuals.widgets.noninteractive.bg_fill = TOP_BAR_MENU_BG;
                visuals.widgets.noninteractive.weak_bg_fill = TOP_BAR_MENU_BG;
                visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER_COLOR);
                visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT_PRIMARY);
                visuals.widgets.inactive.bg_fill = Color32::from_rgb(28, 42, 66);
                visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(28, 42, 66);
                visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, ACCENT_SOFT);
                visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT_PRIMARY);
                visuals.widgets.hovered.bg_fill = Color32::from_rgb(36, 56, 88);
                visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(36, 56, 88);
                visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT_COLOR);
                visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, TEXT_PRIMARY);
                visuals.widgets.active.bg_fill = Color32::from_rgb(42, 66, 103);
                visuals.widgets.active.weak_bg_fill = Color32::from_rgb(42, 66, 103);
                visuals.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT_COLOR);
                visuals.widgets.active.fg_stroke = Stroke::new(1.0, TEXT_PRIMARY);
                visuals.widgets.open.bg_fill = Color32::from_rgb(36, 56, 88);
                visuals.widgets.open.weak_bg_fill = Color32::from_rgb(36, 56, 88);
                visuals.widgets.open.bg_stroke = Stroke::new(1.0, ACCENT_COLOR);
                visuals.widgets.open.fg_stroke = Stroke::new(1.0, TEXT_PRIMARY);

                if ui
                    .add_sized([76.0, 34.0], egui::Button::new("Open"))
                    .clicked()
                {
                    self.start_file_dialog();
                }

                MenuButton::new("Source")
                    .config(self.top_bar_menu_config())
                    .ui(ui, |ui| {
                        self.show_source_menu(ui);
                    });
                MenuButton::new("Navigate")
                    .config(self.top_bar_menu_config())
                    .ui(ui, |ui| {
                        self.show_navigation_menu(ui);
                    });
                MenuButton::new("View")
                    .config(self.top_bar_menu_config())
                    .ui(ui, |ui| {
                        self.show_view_menu(ui);
                    });
                MenuButton::new("Tools")
                    .config(self.top_bar_menu_config())
                    .ui(ui, |ui| {
                        self.show_tools_menu(ui);
                    });
                if ui
                    .add_sized([84.0, 34.0], egui::Button::new("Export"))
                    .clicked()
                {
                    self.show_export_window = true;
                }

                let filter_label = if self.pipeline.is_empty() {
                    "Filters".to_owned()
                } else {
                    format!("Filters ({})", self.pipeline.steps.len())
                };
                MenuButton::new(filter_label)
                    .config(self.top_bar_menu_config())
                    .ui(ui, |ui| {
                        ui.set_min_width(420.0);
                        ScrollArea::vertical()
                            .id_salt("filters-menu-scroll")
                            .max_height(520.0)
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                if self.show_filter_editor(ui) {
                                    self.schedule_rebuild();
                                }
                            });
                    });

                MenuButton::new("Help")
                    .config(self.top_bar_menu_config())
                    .ui(ui, |ui| {
                        self.show_help_menu(ui);
                    });
            });
        });

        if self.file_dialog_pending
            || self.rebuild_pending
            || self.autocorrelation_pending
            || self.export_save_dialog_pending
            || self.export_pending
            || self.document.is_some()
        {
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                if self.file_dialog_pending {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(RichText::new("opening chooser").small().color(TEXT_MUTED));
                    });
                }

                if self.rebuild_pending {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(RichText::new("rebuilding view").small().color(TEXT_MUTED));
                    });
                }

                if self.autocorrelation_pending {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        let label = self
                            .autocorrelation_progress()
                            .map(|(completed, total, fraction)| {
                                format!(
                                    "analyzing widths {} / {} ({:.0}%)",
                                    completed,
                                    total,
                                    fraction * 100.0
                                )
                            })
                            .unwrap_or_else(|| "analyzing widths".to_owned());
                        ui.label(RichText::new(label).small().color(TEXT_MUTED));
                    });
                }

                if self.export_save_dialog_pending {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(
                            RichText::new("choosing export destination")
                                .small()
                                .color(TEXT_MUTED),
                        );
                    });
                }

                if self.export_pending {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(RichText::new("writing export").small().color(TEXT_MUTED));
                    });
                }

                if let Some(document) = &self.document {
                    self.status_chip(ui, document.file_name());
                }
            });
        }

        if let Some(error) = &self.last_error {
            ui.add_space(8.0);
            egui::Frame::new()
                .fill(Color32::from_rgb(58, 22, 28))
                .stroke(Stroke::new(1.0, Color32::from_rgb(108, 45, 52)))
                .corner_radius(CornerRadius::same(12))
                .inner_margin(egui::Margin::symmetric(10, 8))
                .show(ui, |ui| {
                    ui.colored_label(ERROR_COLOR, error);
                });
        }

        if let Some(message) = &self.last_export_message {
            ui.add_space(8.0);
            egui::Frame::new()
                .fill(SUCCESS_BG)
                .stroke(Stroke::new(1.0, SUCCESS_BORDER))
                .corner_radius(CornerRadius::same(12))
                .inner_margin(egui::Margin::symmetric(10, 8))
                .show(ui, |ui| {
                    ui.label(RichText::new(message).color(TEXT_PRIMARY));
                });
        }
    }

    fn show_source_menu(&mut self, ui: &mut Ui) {
        self.apply_top_bar_menu_visuals(ui);
        ui.set_min_width(340.0);
        self.section_header(ui, "Source", "Open a file or paste a path.");

        if ui
            .add_sized([ui.available_width(), 34.0], egui::Button::new("Open file"))
            .clicked()
        {
            self.start_file_dialog();
        }

        if self.file_dialog_pending {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(RichText::new("Opening chooser...").color(TEXT_MUTED));
            });
        }

        ui.separator();
        ui.label(RichText::new("File path").color(TEXT_MUTED));
        let path_response = ui.text_edit_singleline(&mut self.path_input);
        if path_response.lost_focus() && ui.input(|input| input.key_pressed(Key::Enter)) {
            self.open_path_input();
        }
        if ui
            .add_sized([ui.available_width(), 34.0], egui::Button::new("Open path"))
            .clicked()
        {
            self.open_path_input();
        }

        if let Some(document) = &self.document {
            ui.separator();
            ui.label(
                RichText::new(document.path().display().to_string())
                    .monospace()
                    .small()
                    .color(TEXT_MUTED),
            );
        }
    }

    fn show_navigation_menu(&mut self, ui: &mut Ui) {
        self.apply_top_bar_menu_visuals(ui);
        ui.set_min_width(300.0);
        self.section_header(ui, "Navigation", "Jump directly to byte or bit offsets.");

        ui.label(RichText::new("Jump to byte offset").color(TEXT_MUTED));
        let byte_response = ui.text_edit_singleline(&mut self.jump_byte_input);
        if byte_response.lost_focus() && ui.input(|input| input.key_pressed(Key::Enter)) {
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
        if bit_response.lost_focus() && ui.input(|input| input.key_pressed(Key::Enter)) {
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
    }

    fn show_view_menu(&mut self, ui: &mut Ui) {
        self.apply_top_bar_menu_visuals(ui);
        ui.set_min_width(320.0);
        self.section_header(
            ui,
            "View",
            "Tune density and scale without taking focus from the grid.",
        );

        egui::Grid::new("topbar-viewer-settings-grid")
            .num_columns(2)
            .spacing(egui::vec2(12.0, 10.0))
            .show(ui, |ui| {
                ui.label(RichText::new("Row width").color(TEXT_MUTED));
                let row_width_response = ui.add_sized(
                    [96.0, 28.0],
                    egui::TextEdit::singleline(&mut self.row_width_input),
                );
                if row_width_response.changed() {
                    self.apply_row_width_input(false);
                }
                if row_width_response.lost_focus() {
                    self.apply_row_width_input(true);
                }
                ui.end_row();

                ui.label(RichText::new("Bit size").color(TEXT_MUTED));
                let bit_size_response = ui.add(
                    egui::Slider::new(&mut self.target_bit_size, MIN_BIT_SIZE..=MAX_BIT_SIZE)
                        .clamping(egui::SliderClamping::Always)
                        .step_by(1.0),
                );
                if bit_size_response.changed() {
                    self.target_bit_size = self.target_bit_size.clamp(MIN_BIT_SIZE, MAX_BIT_SIZE);
                    self.bit_size = self.target_bit_size;
                }
                ui.end_row();
            });

        if self.target_row_width_bits < MIN_ROW_WIDTH_BITS {
            self.set_row_width_immediately(self.target_row_width_bits);
        }
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
    }

    fn show_tools_menu(&mut self, ui: &mut Ui) {
        self.apply_top_bar_menu_visuals(ui);
        ui.set_min_width(340.0);
        self.section_header(
            ui,
            "Tools",
            "Open analysis panes and configure autocorrelation scans.",
        );

        let button_label = if self.show_autocorrelation_panel {
            "Show autocorrelation"
        } else {
            "Open autocorrelation"
        };
        if ui
            .add_sized(
                [ui.available_width(), 34.0],
                egui::Button::new(button_label),
            )
            .clicked()
        {
            self.show_autocorrelation_panel = true;
        }

        ui.add_space(8.0);
        egui::Grid::new("topbar-tools-grid")
            .num_columns(2)
            .spacing(egui::vec2(12.0, 10.0))
            .show(ui, |ui| {
                ui.label(RichText::new("Corr. max").color(TEXT_MUTED));
                let response = ui.add_sized(
                    [96.0, 28.0],
                    egui::TextEdit::singleline(&mut self.autocorrelation_max_width_input),
                );
                if response.changed() {
                    self.apply_autocorrelation_max_width_input(false);
                }
                if response.lost_focus() {
                    self.apply_autocorrelation_max_width_input(true);
                }
                ui.end_row();

                ui.label(RichText::new("Sample bytes").color(TEXT_MUTED));
                let response = ui.add_sized(
                    [120.0, 28.0],
                    egui::TextEdit::singleline(&mut self.autocorrelation_sample_bytes_input),
                );
                if response.changed() {
                    self.apply_autocorrelation_sample_bytes_input(false);
                }
                if response.lost_focus() {
                    self.apply_autocorrelation_sample_bytes_input(true);
                }
                ui.end_row();
            });

        if self.autocorrelation_pending {
            let progress_text = self
                .autocorrelation_progress()
                .map(|(completed, total, fraction)| {
                    format!(
                        "Analyzing width correlations {} / {} ({:.0}%)",
                        completed,
                        total,
                        fraction * 100.0
                    )
                })
                .unwrap_or_else(|| "Analyzing width correlations".to_owned());
            ui.label(RichText::new(progress_text).small().color(TEXT_MUTED));
        } else if let Some(best_width_bits) = self
            .autocorrelation_result
            .as_ref()
            .and_then(|result| result.best_width_bits)
        {
            let score = self
                .autocorrelation_result
                .as_ref()
                .and_then(|result| result.best_score)
                .unwrap_or_default();
            ui.label(
                RichText::new(format!(
                    "Suggested width {best_width_bits} bits ({score:.3})"
                ))
                .small()
                .color(TEXT_MUTED),
            );
        }
    }

    fn show_help_menu(&mut self, ui: &mut Ui) {
        self.apply_top_bar_menu_visuals(ui);
        ui.set_min_width(260.0);
        self.section_header(
            ui,
            "Help",
            "Shortcuts and viewer interactions stay available in a separate window.",
        );

        if ui
            .add_sized(
                [ui.available_width(), 34.0],
                egui::Button::new("Open shortcuts"),
            )
            .clicked()
        {
            self.show_shortcuts = true;
        }

        ui.separator();
        ui.monospace("[ / ]  row width");
        ui.monospace("- / =  bit size");
        ui.monospace("h      toggle text pane");
        ui.monospace("Home / End / PgUp / PgDn  navigation");
        ui.monospace("Right-click drag in a pane  draw selection");
        ui.monospace("Middle-click in a pane      align all scroll positions");
    }

    fn show_main_content(&mut self, ui: &mut Ui) {
        if let Some(document) = &self.document {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(document.file_name())
                        .size(15.0)
                        .strong()
                        .color(TEXT_PRIMARY),
                );
                self.status_chip(
                    ui,
                    if self.rebuild_pending {
                        "Processing filters"
                    } else {
                        "Ready"
                    },
                );
                self.status_chip(ui, &format!("{} bits/row", self.row_width_bits));
                self.status_chip(ui, &format!("{:.0}px bits", self.bit_size));
                if self.autocorrelation_pending {
                    if let Some((completed, total, fraction)) = self.autocorrelation_progress() {
                        self.status_chip(
                            ui,
                            &format!(
                                "Analyzing widths {} / {} ({:.0}%)",
                                completed,
                                total,
                                fraction * 100.0
                            ),
                        );
                    } else {
                        self.status_chip(ui, "Analyzing widths");
                    }
                } else if let Some(best_width_bits) = self
                    .autocorrelation_result
                    .as_ref()
                    .and_then(|result| result.best_width_bits)
                {
                    self.status_chip(ui, &format!("suggested {} bits", best_width_bits));
                }
                if !self.pipeline.is_empty() {
                    self.status_chip(ui, &format!("{} filter(s)", self.pipeline.steps.len()));
                }
            });
            ui.add_space(10.0);
        }

        self.show_viewer(ui);
    }

    fn show_autocorrelation_pane(&mut self, ui: &mut Ui) {
        let suggested_width_bits = self
            .autocorrelation_result
            .as_ref()
            .and_then(|result| result.best_width_bits);
        let suggested_score = self
            .autocorrelation_result
            .as_ref()
            .and_then(|result| result.best_score);
        let available_width_bits = self
            .autocorrelation_result
            .as_ref()
            .map(|result| result.available_max_width_bits())
            .unwrap_or_default();
        let requested_width_bits = self
            .autocorrelation_result
            .as_ref()
            .map(|result| result.requested_max_width_bits)
            .unwrap_or(self.autocorrelation_max_width_bits);
        let progress_label = self
            .autocorrelation_progress()
            .map(|(completed, total, fraction)| {
                let digits = total.max(1).to_string().len();
                (
                    format!(
                        "Scanning widths {completed:>digits$} / {total:>digits$}",
                        digits = digits
                    ),
                    fraction,
                )
            });
        let mut apply_suggested = false;
        let mut clicked_width_bits = None;

        egui::Frame::new()
            .fill(SURFACE_BG)
            .stroke(Stroke::new(1.0, BORDER_COLOR))
            .inner_margin(egui::Margin::symmetric(16, 14))
            .show(ui, |ui| {
                ui.set_min_width(AUTOCORRELATION_WINDOW_WIDTH - 72.0);
                ui.set_max_width(AUTOCORRELATION_WINDOW_WIDTH - 72.0);
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        self.viewer_pane_header(
                            ui,
                            "Autocorrelation",
                            "click the graph to apply a row width",
                        );
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if let Some(width_bits) = suggested_width_bits
                            && ui.button(format!("Apply {width_bits} bits")).clicked()
                        {
                            apply_suggested = true;
                        }
                    });
                });

                ui.add_space(8.0);
                ui.horizontal_wrapped(|ui| {
                    self.compact_status_chip(ui, &format!("max {} bits", requested_width_bits));
                    self.compact_status_chip(
                        ui,
                        &format!("sample {} bytes", self.autocorrelation_sample_bytes),
                    );
                    self.compact_status_chip(ui, &format!("current {} bits", self.row_width_bits));
                    if available_width_bits > 0 && available_width_bits < requested_width_bits {
                        self.compact_status_chip(
                            ui,
                            &format!("scanned {} bits", available_width_bits),
                        );
                    }
                    if let Some(width_bits) = suggested_width_bits {
                        self.compact_status_chip(ui, &format!("best {} bits", width_bits));
                    }
                    if let Some(score) = suggested_score {
                        self.compact_status_chip(ui, &format!("score {score:.3}"));
                    }
                });

                if self.autocorrelation_pending
                    && let Some((label, fraction)) = progress_label.as_ref()
                {
                    ui.add_space(10.0);
                    ui.add(
                        egui::ProgressBar::new(*fraction)
                            .show_percentage()
                            .text(label),
                    );
                }

                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Corr. max").color(TEXT_MUTED));
                    let response = ui.add_sized(
                        [96.0, 28.0],
                        egui::TextEdit::singleline(&mut self.autocorrelation_max_width_input),
                    );
                    if response.changed() {
                        self.apply_autocorrelation_max_width_input(false);
                    }
                    if response.lost_focus() {
                        self.apply_autocorrelation_max_width_input(true);
                    }
                    ui.add_space(12.0);
                    ui.label(RichText::new("Sample bytes").color(TEXT_MUTED));
                    let response = ui.add_sized(
                        [120.0, 28.0],
                        egui::TextEdit::singleline(&mut self.autocorrelation_sample_bytes_input),
                    );
                    if response.changed() {
                        self.apply_autocorrelation_sample_bytes_input(false);
                    }
                    if response.lost_focus() {
                        self.apply_autocorrelation_sample_bytes_input(true);
                    }
                });

                ui.add_space(10.0);
                egui::Frame::new()
                    .fill(SURFACE_ALT_BG)
                    .stroke(Stroke::new(1.0, BORDER_COLOR))
                    .corner_radius(CornerRadius::same(14))
                    .inner_margin(egui::Margin::symmetric(12, 10))
                    .show(ui, |ui| {
                        if self.autocorrelation_pending {
                            ui.vertical_centered(|ui| {
                                ui.add_space(28.0);
                                ui.spinner();
                                ui.add_space(8.0);
                                ui.label(
                                    RichText::new("Computing width correlations")
                                        .small()
                                        .color(TEXT_MUTED),
                                );
                                ui.add_space(28.0);
                            });
                        } else if let Some(result) = self.autocorrelation_result.as_ref() {
                            clicked_width_bits =
                                paint_autocorrelation_graph(ui, result, self.row_width_bits);
                        } else {
                            ui.vertical_centered(|ui| {
                                ui.add_space(28.0);
                                ui.label(
                                    RichText::new(
                                        "No autocorrelation data is available for this view.",
                                    )
                                    .small()
                                    .color(TEXT_MUTED),
                                );
                                ui.add_space(28.0);
                            });
                        }
                    });
            });

        if apply_suggested && let Some(width_bits) = suggested_width_bits {
            self.set_row_width_immediately(width_bits);
        }

        if let Some(width_bits) = clicked_width_bits {
            self.set_row_width_immediately(width_bits);
        }
    }

    fn show_autocorrelation_window(&mut self, context: &Context) {
        if !self.show_autocorrelation_panel {
            return;
        }

        let mut show_autocorrelation_panel = self.show_autocorrelation_panel;
        egui::Window::new("Autocorrelation")
            .open(&mut show_autocorrelation_panel)
            .default_width(AUTOCORRELATION_WINDOW_WIDTH)
            .default_height(AUTOCORRELATION_WINDOW_DEFAULT_HEIGHT)
            .min_width(AUTOCORRELATION_WINDOW_WIDTH)
            .max_width(AUTOCORRELATION_WINDOW_WIDTH)
            .min_height(AUTOCORRELATION_WINDOW_MIN_HEIGHT)
            .frame(
                egui::Frame::new()
                    .fill(SURFACE_BG)
                    .stroke(Stroke::new(1.0, BORDER_COLOR))
                    .corner_radius(CornerRadius::same(20))
                    .inner_margin(egui::Margin::symmetric(18, 16)),
            )
            .show(context, |ui| {
                self.show_autocorrelation_pane(ui);
            });
        self.show_autocorrelation_panel = show_autocorrelation_panel;
    }

    fn show_filter_editor(&mut self, ui: &mut Ui) -> bool {
        self.apply_top_bar_menu_visuals(ui);
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
            self.compact_card_frame(SURFACE_ALT_BG).show(ui, |ui| {
                ui.horizontal(|ui| {
                    self.compact_status_chip(ui, &format!("{:02}", index + 1));
                    ui.label(RichText::new(self.pipeline.steps[index].label()).strong());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.small_button("Delete").clicked() {
                            delete = Some(index);
                        }
                        if ui.small_button("Down").clicked()
                            && index + 1 < self.pipeline.steps.len()
                        {
                            move_down = Some(index);
                        }
                        if ui.small_button("Up").clicked() && index > 0 {
                            move_up = Some(index);
                        }
                    });
                });

                let step = &mut self.pipeline.steps[index];
                match step {
                    FilterStep::SyncOnPreamble { bits } => {
                        ui.add_space(6.0);
                        let response = ui.horizontal(|ui| {
                            ui.label(RichText::new("Preamble bits").color(TEXT_MUTED));
                            ui.add_space(6.0);
                            ui.add_sized(
                                [ui.available_width(), 28.0],
                                egui::TextEdit::singleline(bits).hint_text("1010 or 0xA"),
                            )
                        });
                        if response.inner.changed() {
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
                self.status_chip(ui, &document.source_size_label());
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

                if self.autocorrelation_pending
                    && let Some((completed, total, fraction)) = self.autocorrelation_progress()
                {
                    ui.spinner();
                    ui.label(
                        RichText::new(format!(
                            "analyzing widths {} / {} ({:.0}%)",
                            completed,
                            total,
                            fraction * 100.0
                        ))
                        .color(TEXT_MUTED),
                    );
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
                            "If the native chooser stalls, use the Source menu in the top bar or drag a file into the window.",
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
                ui.separator();
                ui.label(RichText::new("Viewer").strong());
                egui::Grid::new("viewer-shortcuts-grid")
                    .num_columns(2)
                    .spacing(egui::vec2(16.0, 10.0))
                    .show(ui, |ui| {
                        ui.monospace("Right-click drag in bit / hex / ASCII");
                        ui.label(
                            RichText::new("Draw or erase highlighted columns").color(TEXT_MUTED),
                        );
                        ui.end_row();

                        ui.monospace("Middle-click in bit / hex / ASCII");
                        ui.label(
                            RichText::new("Align all panes to the clicked pane's row")
                                .color(TEXT_MUTED),
                        );
                        ui.end_row();
                    });
            });
        self.show_shortcuts = show_shortcuts;
    }

    fn show_export_window(&mut self, context: &Context) {
        if !self.show_export_window {
            return;
        }

        let mut show_export_window = self.show_export_window;
        egui::Window::new("Export")
            .open(&mut show_export_window)
            .default_width(720.0)
            .min_width(620.0)
            .frame(
                egui::Frame::new()
                    .fill(SURFACE_BG)
                    .stroke(Stroke::new(1.0, BORDER_COLOR))
                    .corner_radius(CornerRadius::same(20))
                    .inner_margin(egui::Margin::symmetric(18, 16)),
            )
            .show(context, |ui| {
                self.section_header(
                    ui,
                    "Export View",
                    "Write the current derived view as raw bits, a packet capture, or a WAV container.",
                );

                if let Some(view) = &self.derived_view {
                    ui.horizontal_wrapped(|ui| {
                        self.status_chip(ui, &format!("{} groups", view.group_count()));
                        self.status_chip(ui, &format!("{} bits", view.total_bits()));
                        self.status_chip(
                            ui,
                            &format!("{} rounded bytes", view.total_bytes_rounded_up()),
                        );
                    });

                    if view.total_bits() % 8 != 0 {
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new(
                                "The derived view ends on a partial byte. Export pads the tail with zero bits.",
                            )
                            .small()
                            .color(TEXT_MUTED),
                        );
                    }
                } else if self.rebuild_pending {
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(
                            "Wait for the filtered view to finish building before exporting.",
                        )
                        .color(TEXT_MUTED),
                    );
                } else {
                    ui.add_space(8.0);
                    ui.label(RichText::new("Load a file before exporting.").color(TEXT_MUTED));
                }

                ui.add_space(10.0);
                egui::Grid::new("export-settings-grid")
                    .num_columns(2)
                    .spacing(egui::vec2(14.0, 10.0))
                    .show(ui, |ui| {
                        ui.label(RichText::new("Format").color(TEXT_MUTED));
                        egui::ComboBox::from_id_salt("export-format")
                            .selected_text(self.export_format_label())
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.export_format,
                                    ExportFormat::FlattenedBits,
                                    "Flattened bit file",
                                );
                                ui.selectable_value(
                                    &mut self.export_format,
                                    ExportFormat::Pcap,
                                    "PCAP packet capture",
                                );
                                ui.selectable_value(
                                    &mut self.export_format,
                                    ExportFormat::Wav,
                                    "WAV audio file",
                                );
                            });
                        ui.end_row();
                    });

                ui.add_space(12.0);
                match self.export_format {
                    ExportFormat::FlattenedBits => self.show_flattened_export_ui(ui),
                    ExportFormat::Pcap => self.show_pcap_export_ui(ui),
                    ExportFormat::Wav => self.show_wav_export_ui(ui),
                }

                ui.add_space(14.0);
                if ui
                    .add_enabled(
                        !self.export_save_dialog_pending && !self.export_pending,
                        egui::Button::new("Choose destination and export")
                            .min_size(egui::vec2(240.0, 38.0)),
                    )
                    .clicked()
                {
                    self.start_export_flow();
                }

                if self.export_save_dialog_pending || self.export_pending {
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.spinner();
                        let status = if self.export_pending {
                            "Writing export..."
                        } else {
                            "Waiting for destination..."
                        };
                        ui.label(RichText::new(status).color(TEXT_MUTED));
                    });
                }
            });

        self.show_export_window = show_export_window;
    }

    fn show_flattened_export_ui(&self, ui: &mut Ui) {
        self.compact_card_frame(SURFACE_ALT_BG).show(ui, |ui| {
            self.section_header(
                ui,
                "Flattened Bit File",
                "Concatenate all visible groups and write the packed bitstream as raw bytes.",
            );
            ui.label(
                RichText::new(
                    "The output uses the same MSB-first bit packing as the viewer and zero-pads the final byte when needed.",
                )
                .small()
                .color(TEXT_MUTED),
            );
        });
    }

    fn show_pcap_export_ui(&mut self, ui: &mut Ui) {
        self.compact_card_frame(SURFACE_ALT_BG).show(ui, |ui| {
            self.section_header(
                ui,
                "PCAP",
                "Export each visible group as a packet. Search the full known link-layer catalog or switch to a custom numeric type.",
            );

            ui.checkbox(
                &mut self.pcap_use_custom_link_type,
                "Use a custom numeric link-layer type",
            );
            ui.add_space(6.0);

            if self.pcap_use_custom_link_type {
                ui.scope(|ui| {
                    let visuals = &mut ui.style_mut().visuals;
                    visuals.override_text_color = Some(TEXT_PRIMARY);
                    visuals.widgets.inactive.bg_fill = SURFACE_SUBTLE_BG;
                    visuals.widgets.inactive.weak_bg_fill = SURFACE_SUBTLE_BG;
                    visuals.widgets.hovered.bg_fill = Color32::from_rgb(37, 50, 76);
                    visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(37, 50, 76);
                    visuals.widgets.active.bg_fill = Color32::from_rgb(43, 59, 88);
                    visuals.widgets.active.weak_bg_fill = Color32::from_rgb(43, 59, 88);

                    egui::Grid::new("pcap-custom-linktype-grid")
                        .num_columns(2)
                        .spacing(egui::vec2(12.0, 8.0))
                        .show(ui, |ui| {
                            ui.label(RichText::new("Link type id").color(TEXT_MUTED));
                            ui.add(
                                egui::DragValue::new(&mut self.pcap_custom_link_type)
                                    .range(0..=u32::MAX)
                                    .speed(1),
                            );
                            ui.end_row();
                        });
                });
            } else {
                ui.label(RichText::new("Search link-layer type").color(TEXT_MUTED));
                ui.scope(|ui| {
                    let visuals = &mut ui.style_mut().visuals;
                    visuals.override_text_color = Some(TEXT_PRIMARY);
                    visuals.widgets.inactive.bg_fill = SURFACE_SUBTLE_BG;
                    visuals.widgets.inactive.weak_bg_fill = SURFACE_SUBTLE_BG;
                    visuals.widgets.hovered.bg_fill = Color32::from_rgb(37, 50, 76);
                    visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(37, 50, 76);
                    visuals.widgets.active.bg_fill = Color32::from_rgb(43, 59, 88);
                    visuals.widgets.active.weak_bg_fill = Color32::from_rgb(43, 59, 88);

                    ui.add(
                        egui::TextEdit::singleline(&mut self.pcap_link_search).hint_text(
                            "Search by numeric id or name, for example 105 or IEEE802_11",
                        ),
                    );
                });
                ui.add_space(8.0);

                let query = self.pcap_link_search.trim().to_ascii_lowercase();
                let mut matches = 0usize;
                ui.scope(|ui| {
                    let visuals = &mut ui.style_mut().visuals;
                    visuals.override_text_color = Some(TEXT_PRIMARY);
                    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT_PRIMARY);
                    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, TEXT_PRIMARY);
                    visuals.widgets.active.fg_stroke = Stroke::new(1.0, TEXT_PRIMARY);

                    ScrollArea::vertical()
                        .id_salt("pcap-linktype-search-results")
                        .max_height(240.0)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            for option in self.link_type_options.iter().filter(|option| {
                                query.is_empty()
                                    || option.id.to_string().contains(&query)
                                    || option.label.to_ascii_lowercase().contains(&query)
                            }) {
                                matches += 1;
                                let is_selected = self.selected_link_type == option.id;
                                if ui
                                    .add(
                                        egui::Button::new(
                                            RichText::new(format!(
                                                "{:>3}  {}",
                                                option.id, option.label
                                            ))
                                            .color(TEXT_PRIMARY)
                                            .monospace(),
                                        )
                                        .selected(is_selected)
                                        .fill(if is_selected {
                                            Color32::from_rgb(45, 68, 102)
                                        } else {
                                            SURFACE_SUBTLE_BG
                                        })
                                        .stroke(Stroke::new(
                                            1.0,
                                            if is_selected {
                                                ACCENT_COLOR
                                            } else {
                                                BORDER_COLOR
                                            },
                                        )),
                                    )
                                    .clicked()
                                {
                                    self.selected_link_type = option.id;
                                }
                            }
                        });
                });

                if matches == 0 {
                    ui.label(
                        RichText::new("No link-layer types matched the current search.")
                            .small()
                            .color(TEXT_MUTED),
                    );
                } else {
                    ui.label(
                        RichText::new(format!("{matches} known link-layer types matched."))
                            .small()
                            .color(TEXT_MUTED),
                    );
                }
            }

            ui.add_space(8.0);
            ui.scope(|ui| {
                let visuals = &mut ui.style_mut().visuals;
                visuals.override_text_color = Some(TEXT_PRIMARY);
                visuals.widgets.inactive.bg_fill = SURFACE_SUBTLE_BG;
                visuals.widgets.inactive.weak_bg_fill = SURFACE_SUBTLE_BG;
                visuals.widgets.hovered.bg_fill = Color32::from_rgb(37, 50, 76);
                visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(37, 50, 76);
                visuals.widgets.active.bg_fill = Color32::from_rgb(43, 59, 88);
                visuals.widgets.active.weak_bg_fill = Color32::from_rgb(43, 59, 88);

                egui::Grid::new("pcap-export-grid")
                    .num_columns(2)
                    .spacing(egui::vec2(12.0, 8.0))
                    .show(ui, |ui| {
                        ui.label(RichText::new("Timestamp step").color(TEXT_MUTED));
                        ui.add(
                            egui::DragValue::new(&mut self.pcap_timestamp_step_micros)
                                .range(1..=u32::MAX)
                                .speed(1)
                                .suffix(" us"),
                        );
                        ui.end_row();
                    });
            });
        });
    }

    fn show_wav_export_ui(&mut self, ui: &mut Ui) {
        self.compact_card_frame(SURFACE_ALT_BG).show(ui, |ui| {
            self.section_header(
                ui,
                "WAV",
                "Wrap the flattened byte stream in RIFF/WAVE using PCM, IEEE float, A-LAW, or mu-LAW format tags.",
            );

            egui::Grid::new("wav-export-grid")
                .num_columns(2)
                .spacing(egui::vec2(12.0, 10.0))
                .show(ui, |ui| {
                    ui.label(RichText::new("Codec preset").color(TEXT_MUTED));
                    egui::ComboBox::from_id_salt("wav-codec")
                        .selected_text(self.wav_options.codec.label())
                        .show_ui(ui, |ui| {
                            for codec in WAV_CODEC_PRESETS {
                                ui.selectable_value(
                                    &mut self.wav_options.codec,
                                    codec,
                                    codec.label(),
                                );
                            }
                        });
                    ui.end_row();

                    ui.label(RichText::new("Sample rate").color(TEXT_MUTED));
                    ui.horizontal_wrapped(|ui| {
                        egui::ComboBox::from_id_salt("wav-sample-rate")
                            .selected_text(self.wav_options.sample_rate.to_string())
                            .show_ui(ui, |ui| {
                                for sample_rate in WAV_SAMPLE_RATE_PRESETS {
                                    ui.selectable_value(
                                        &mut self.wav_options.sample_rate,
                                        sample_rate,
                                        sample_rate.to_string(),
                                    );
                                }
                            });
                        ui.add(
                            egui::DragValue::new(&mut self.wav_options.sample_rate)
                                .range(1..=u32::MAX)
                                .speed(100),
                        );
                    });
                    ui.end_row();

                    ui.label(RichText::new("Channels").color(TEXT_MUTED));
                    ui.horizontal(|ui| {
                        egui::ComboBox::from_id_salt("wav-channels")
                            .selected_text(self.wav_options.channels.to_string())
                            .show_ui(ui, |ui| {
                                for channels in 1..=MAX_WAV_CHANNELS {
                                    ui.selectable_value(
                                        &mut self.wav_options.channels,
                                        channels,
                                        channels.to_string(),
                                    );
                                }
                            });
                        ui.add(
                            egui::DragValue::new(&mut self.wav_options.channels)
                                .range(1..=MAX_WAV_CHANNELS)
                                .speed(1),
                        );
                    });
                    ui.end_row();
                });

            ui.add_space(6.0);
            ui.label(
                RichText::new(
                    "The flattened byte stream becomes interleaved sample frames. Multi-byte codecs require the payload length to align to the chosen frame size.",
                )
                .small()
                .color(TEXT_MUTED),
            );
        });
    }

    fn export_format_label(&self) -> &'static str {
        match self.export_format {
            ExportFormat::FlattenedBits => "Flattened bit file",
            ExportFormat::Pcap => "PCAP packet capture",
            ExportFormat::Wav => "WAV audio file",
        }
    }

    fn card_frame(&self, fill: Color32) -> egui::Frame {
        egui::Frame::new()
            .fill(fill)
            .stroke(Stroke::new(1.0, BORDER_COLOR))
            .corner_radius(CornerRadius::same(18))
            .inner_margin(18)
    }

    fn compact_card_frame(&self, fill: Color32) -> egui::Frame {
        egui::Frame::new()
            .fill(fill)
            .stroke(Stroke::new(1.0, BORDER_COLOR))
            .corner_radius(CornerRadius::same(14))
            .inner_margin(egui::Margin::symmetric(12, 10))
    }

    fn apply_top_bar_menu_visuals(&self, ui: &mut Ui) {
        let visuals = &mut ui.style_mut().visuals;
        visuals.override_text_color = Some(TEXT_PRIMARY);
        visuals.panel_fill = TOP_BAR_MENU_BG;
        visuals.window_fill = TOP_BAR_MENU_BG;
        visuals.faint_bg_color = TOP_BAR_MENU_ALT_BG;
        visuals.extreme_bg_color = TOP_BAR_MENU_ALT_BG;
        visuals.widgets.inactive.bg_fill = TOP_BAR_MENU_ALT_BG;
        visuals.widgets.inactive.weak_bg_fill = TOP_BAR_MENU_ALT_BG;
        visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER_COLOR);
        visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT_PRIMARY);
        visuals.widgets.hovered.bg_fill = Color32::from_rgb(42, 66, 103);
        visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(42, 66, 103);
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT_COLOR);
        visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, TEXT_PRIMARY);
        visuals.widgets.active.bg_fill = Color32::from_rgb(48, 74, 115);
        visuals.widgets.active.weak_bg_fill = Color32::from_rgb(48, 74, 115);
        visuals.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT_COLOR);
        visuals.widgets.active.fg_stroke = Stroke::new(1.0, TEXT_PRIMARY);
        visuals.widgets.open.bg_fill = Color32::from_rgb(42, 66, 103);
        visuals.widgets.open.weak_bg_fill = Color32::from_rgb(42, 66, 103);
        visuals.widgets.open.bg_stroke = Stroke::new(1.0, ACCENT_COLOR);
        visuals.widgets.open.fg_stroke = Stroke::new(1.0, TEXT_PRIMARY);
    }

    fn top_bar_menu_config(&self) -> MenuConfig {
        MenuConfig::new().close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
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

    fn compact_status_chip(&self, ui: &mut Ui, label: &str) {
        egui::Frame::new()
            .fill(SURFACE_SUBTLE_BG)
            .stroke(Stroke::new(1.0, ACCENT_SOFT))
            .corner_radius(CornerRadius::same(255))
            .inner_margin(egui::Margin::symmetric(7, 3))
            .show(ui, |ui| {
                ui.label(RichText::new(label).small().color(TEXT_PRIMARY));
            });
    }

    fn viewer_pane_header(&self, ui: &mut Ui, title: &str, detail: &str) {
        ui.horizontal(|ui| {
            ui.label(RichText::new(title).strong().color(TEXT_PRIMARY));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(RichText::new(detail).small().color(TEXT_MUTED));
            });
        });
        ui.add_space(8.0);
    }

    fn sync_scroll_positions_on_middle_click(&mut self, ui: &Ui, rect: Rect, row: usize) {
        let should_sync = ui.ctx().input(|input| {
            input.pointer.button_clicked(PointerButton::Middle)
                && input
                    .pointer
                    .interact_pos()
                    .is_some_and(|pointer_pos| rect.contains(pointer_pos))
        });

        if should_sync {
            self.pending_bit_scroll_to_row = Some(row);
            self.pending_text_scroll_to_row = Some(row);
            ui.ctx().request_repaint();
        }
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
        let text_char_width = text_pane_char_width(ui);
        let hex_content_width =
            text_pane_content_width(bytes_per_row, TextPaneKind::Hex, text_char_width);
        let hex_width = HEX_COLUMN_MIN_WIDTH.max(hex_content_width);
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
            let max_text_panel_width =
                (ui.available_width() * TEXT_PANEL_MAX_SHARE).max(TEXT_PANEL_MIN_WIDTH);
            let default_text_panel_width = (hex_width + ASCII_COLUMN_DEFAULT_WIDTH + 36.0)
                .clamp(TEXT_PANEL_MIN_WIDTH, max_text_panel_width);

            egui::SidePanel::right("text-pane-group")
                .resizable(true)
                .show_separator_line(true)
                .default_width(default_text_panel_width)
                .min_width(TEXT_PANEL_MIN_WIDTH)
                .frame(egui::Frame::new().inner_margin(egui::Margin::symmetric(0, 0)))
                .show_inside(ui, |ui| {
                    observed_text_scroll_row = self.show_text_panes(
                        ui,
                        layout.as_ref(),
                        total_rows,
                        available_height,
                        text_row_height,
                        text_scroll_target_row,
                        bytes_per_row,
                        text_char_width,
                        hex_width,
                    );
                });

            egui::CentralPanel::default()
                .frame(
                    egui::Frame::new()
                        .fill(SURFACE_ALT_BG)
                        .inner_margin(egui::Margin::symmetric(0, 0)),
                )
                .show_inside(ui, |ui| {
                    observed_bit_scroll_row = self.show_bit_grid_panel(
                        ui,
                        layout.as_ref(),
                        total_rows,
                        available_height,
                        bit_panel_width,
                        bit_content_height,
                        bit_row_height,
                        pending_bit_scroll_to_row,
                        bit_overscan_rows,
                        bit_overscan_cols,
                    );
                });

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
                    observed_bit_scroll_row = self.show_bit_grid_panel(
                        ui,
                        layout.as_ref(),
                        total_rows,
                        available_height,
                        bit_panel_width,
                        bit_content_height,
                        bit_row_height,
                        pending_bit_scroll_to_row,
                        bit_overscan_rows,
                        bit_overscan_cols,
                    );
                });
        }

        self.current_bit_scroll_row = observed_bit_scroll_row.min(total_rows.saturating_sub(1));
        self.current_text_scroll_row = observed_text_scroll_row.min(total_rows.saturating_sub(1));
    }

    fn show_bit_grid_panel(
        &mut self,
        ui: &mut Ui,
        layout: &RowLayout,
        total_rows: usize,
        available_height: f32,
        bit_panel_width: f32,
        bit_content_height: f32,
        bit_row_height: f32,
        pending_bit_scroll_to_row: Option<usize>,
        bit_overscan_rows: usize,
        bit_overscan_cols: usize,
    ) -> usize {
        let mut observed_bit_scroll_row = self.current_bit_scroll_row;

        egui::Frame::new()
            .fill(SURFACE_BG)
            .stroke(Stroke::new(1.0, BORDER_COLOR))
            .corner_radius(CornerRadius::same(18))
            .inner_margin(egui::Margin::symmetric(16, 14))
            .show(ui, |ui| {
                ui.set_min_height(available_height);
                ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
                self.viewer_pane_header(
                    ui,
                    "Bit Grid",
                    &format!("{} bits per row", self.row_width_bits),
                );

                let pane_height = ui.available_height();
                let mut scroll_area = ScrollArea::both()
                    .id_salt("native-bit-scroll")
                    .auto_shrink([false, false])
                    .max_height(pane_height)
                    .min_scrolled_height(pane_height)
                    .wheel_scroll_multiplier(SCROLL_MULTIPLIER);

                if let Some(target_row) = pending_bit_scroll_to_row {
                    scroll_area =
                        scroll_area.vertical_scroll_offset(target_row as f32 * bit_row_height);
                }

                let output = scroll_area.show_viewport(ui, |ui, viewport| {
                    let content_width = bit_panel_width
                        .max(BIT_PANEL_MIN_WIDTH)
                        .min(BIT_PANEL_DEFAULT_MAX_WIDTH)
                        .max(bit_panel_width);
                    ui.set_min_size(Vec2::new(content_width, bit_content_height));
                    let content_rect = Rect::from_min_size(
                        ui.max_rect().min,
                        Vec2::new(content_width, bit_content_height),
                    );

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
                    let highlighted_bit_ranges = self.highlighted_bit_ranges();

                    if let Some(texture_id) = self.ensure_bit_texture(
                        ui.ctx(),
                        layout,
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
                        paint_column_drag_overlay(
                            ui,
                            content_rect,
                            self.bit_size,
                            self.row_width_bits,
                            highlighted_bit_ranges.as_slice(),
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
                self.handle_draw_input(
                    ui,
                    output.inner_rect,
                    DrawGranularity::Bit,
                    pointer_bit_col_in_bit_grid(
                        output.inner_rect,
                        output.state.offset.x,
                        self.bit_size,
                        self.row_width_bits,
                        ui.ctx().input(|input| input.pointer.interact_pos()),
                    ),
                );
                self.sync_scroll_positions_on_middle_click(
                    ui,
                    output.inner_rect,
                    observed_bit_scroll_row,
                );
            });

        observed_bit_scroll_row
    }

    fn show_text_panes(
        &mut self,
        ui: &mut Ui,
        layout: &RowLayout,
        total_rows: usize,
        available_height: f32,
        text_row_height: f32,
        text_scroll_target_row: usize,
        bytes_per_row: usize,
        text_char_width: f32,
        hex_width: f32,
    ) -> usize {
        ui.set_min_height(available_height);
        ui.spacing_mut().item_spacing = egui::vec2(VIEWER_PANEL_GAP, 0.0);

        let total_width = ui.available_width();
        let ascii_panel_width = ASCII_COLUMN_DEFAULT_WIDTH
            .max(ASCII_COLUMN_MIN_WIDTH)
            .min((total_width * 0.38).max(ASCII_COLUMN_MIN_WIDTH));
        let hex_panel_width =
            (total_width - ascii_panel_width - VIEWER_PANEL_GAP).max(HEX_COLUMN_MIN_WIDTH);
        let ascii_content_width = ASCII_COLUMN_MIN_WIDTH.max(text_pane_content_width(
            bytes_per_row,
            TextPaneKind::Ascii,
            text_char_width,
        ));
        let mut hex_observed_row = self.current_text_scroll_row;
        let mut ascii_observed_row = self.current_text_scroll_row;

        ui.horizontal(|ui| {
            ui.allocate_ui_with_layout(
                Vec2::new(hex_panel_width, available_height),
                Layout::top_down(Align::Min),
                |ui| {
                    egui::Frame::new()
                        .fill(SURFACE_BG)
                        .stroke(Stroke::new(1.0, BORDER_COLOR))
                        .corner_radius(CornerRadius::same(18))
                        .inner_margin(egui::Margin::symmetric(14, 14))
                        .show(ui, |ui| {
                            self.viewer_pane_header(
                                ui,
                                "Hex",
                                &format!("{bytes_per_row} bytes per row"),
                            );

                            let pane_height = ui.available_height();
                            let pane_width = ui.available_width().max(hex_width);
                            let scroll_area = ScrollArea::both()
                                .id_salt("native-hex-scroll")
                                .auto_shrink([false, false])
                                .max_height(pane_height)
                                .min_scrolled_height(pane_height)
                                .wheel_scroll_multiplier(SCROLL_MULTIPLIER)
                                .vertical_scroll_offset(
                                    text_scroll_target_row as f32 * text_row_height,
                                );
                            let highlighted_byte_ranges = self.highlighted_byte_ranges();

                            let output = scroll_area.show_rows(
                                ui,
                                text_row_height,
                                total_rows,
                                |ui, row_range| {
                                    let start = row_range.start.saturating_sub(TEXT_OVERSCAN_ROWS);
                                    let end = (row_range.end + TEXT_OVERSCAN_ROWS).min(total_rows);

                                    for (row_offset, row) in self
                                        .text_rows(layout, start, end.saturating_sub(start))
                                        .iter()
                                        .enumerate()
                                    {
                                        paint_single_text_row(
                                            ui,
                                            &row.hex,
                                            text_row_height,
                                            pane_width.max(hex_width),
                                            TEXT_PRIMARY,
                                            start + row_offset,
                                            TextPaneKind::Hex,
                                            text_char_width,
                                            highlighted_byte_ranges.as_slice(),
                                        );
                                    }
                                },
                            );
                            hex_observed_row =
                                (output.state.offset.y / text_row_height).floor().max(0.0) as usize;
                            self.handle_draw_input(
                                ui,
                                output.inner_rect,
                                DrawGranularity::Byte,
                                pointer_byte_col_in_text_pane(
                                    output.inner_rect,
                                    output.state.offset.x,
                                    bytes_per_row,
                                    TextPaneKind::Hex,
                                    text_char_width,
                                    ui.ctx().input(|input| input.pointer.interact_pos()),
                                ),
                            );
                            self.sync_scroll_positions_on_middle_click(
                                ui,
                                output.inner_rect,
                                hex_observed_row,
                            );
                        });
                },
            );

            ui.allocate_ui_with_layout(
                Vec2::new(ascii_panel_width, available_height),
                Layout::top_down(Align::Min),
                |ui| {
                    egui::Frame::new()
                        .fill(SURFACE_BG)
                        .stroke(Stroke::new(1.0, BORDER_COLOR))
                        .corner_radius(CornerRadius::same(18))
                        .inner_margin(egui::Margin::symmetric(14, 14))
                        .show(ui, |ui| {
                            self.viewer_pane_header(ui, "ASCII", "printable preview");

                            let pane_height = ui.available_height();
                            let pane_width = ui.available_width().max(ascii_content_width);
                            let scroll_area = ScrollArea::both()
                                .id_salt("native-ascii-scroll")
                                .auto_shrink([false, false])
                                .max_height(pane_height)
                                .min_scrolled_height(pane_height)
                                .wheel_scroll_multiplier(SCROLL_MULTIPLIER)
                                .vertical_scroll_offset(
                                    text_scroll_target_row as f32 * text_row_height,
                                );
                            let highlighted_byte_ranges = self.highlighted_byte_ranges();

                            let output = scroll_area.show_rows(
                                ui,
                                text_row_height,
                                total_rows,
                                |ui, row_range| {
                                    let start = row_range.start.saturating_sub(TEXT_OVERSCAN_ROWS);
                                    let end = (row_range.end + TEXT_OVERSCAN_ROWS).min(total_rows);

                                    for (row_offset, row) in self
                                        .text_rows(layout, start, end.saturating_sub(start))
                                        .iter()
                                        .enumerate()
                                    {
                                        paint_single_text_row(
                                            ui,
                                            &row.ascii,
                                            text_row_height,
                                            pane_width,
                                            TEXT_PRIMARY,
                                            start + row_offset,
                                            TextPaneKind::Ascii,
                                            text_char_width,
                                            highlighted_byte_ranges.as_slice(),
                                        );
                                    }
                                },
                            );
                            ascii_observed_row =
                                (output.state.offset.y / text_row_height).floor().max(0.0) as usize;
                            self.handle_draw_input(
                                ui,
                                output.inner_rect,
                                DrawGranularity::Byte,
                                pointer_byte_col_in_text_pane(
                                    output.inner_rect,
                                    output.state.offset.x,
                                    bytes_per_row,
                                    TextPaneKind::Ascii,
                                    text_char_width,
                                    ui.ctx().input(|input| input.pointer.interact_pos()),
                                ),
                            );
                            self.sync_scroll_positions_on_middle_click(
                                ui,
                                output.inner_rect,
                                ascii_observed_row,
                            );
                        });
                },
            );
        });

        if hex_observed_row != text_scroll_target_row {
            hex_observed_row
        } else {
            ascii_observed_row
        }
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

    fn start_export_flow(&mut self) {
        if self.export_save_dialog_pending || self.export_pending {
            return;
        }

        let Some(request) = self.build_pending_export_request() else {
            return;
        };
        let Some(document) = &self.document else {
            self.last_error = Some("Load a file before exporting.".to_owned());
            return;
        };

        let suggested_name = default_export_file_name(document.file_name(), request.format);
        let format = request.format;
        let request_id = request.request_id;
        let (sender, receiver) = mpsc::channel();

        self.export_save_dialog_rx = Some(receiver);
        self.export_save_dialog_pending = true;
        self.pending_export_request = Some(request);
        self.last_error = None;
        self.last_export_message = None;

        thread::spawn(move || {
            let dialog = FileDialog::new()
                .set_file_name(&suggested_name)
                .add_filter(format.filter_label(), &[format.default_extension()]);
            let path = dialog.save_file();
            let _ = sender.send(ExportSaveDialogResult { request_id, path });
        });
    }

    fn build_pending_export_request(&mut self) -> Option<PendingExportRequest> {
        if self.rebuild_pending {
            self.last_error = Some("Wait for the filtered view to finish building.".to_owned());
            return None;
        }

        let Some(view) = self.derived_view.clone() else {
            self.last_error = Some("There is no derived view to export.".to_owned());
            return None;
        };

        let pcap_link_type = if self.pcap_use_custom_link_type {
            self.pcap_custom_link_type
        } else {
            self.selected_link_type
        };
        let pcap = PcapExportOptions {
            link_type: pcap_link_type,
            timestamp_step_micros: self.pcap_timestamp_step_micros.max(1),
        };
        let wav = self.wav_options.clone();
        let request_id = self.export_request_id.saturating_add(1);
        self.export_request_id = request_id;

        Some(PendingExportRequest {
            request_id,
            format: self.export_format,
            view,
            pcap,
            wav,
        })
    }

    fn poll_export_save_dialog(&mut self) {
        let Some(receiver) = &self.export_save_dialog_rx else {
            return;
        };

        match receiver.try_recv() {
            Ok(result) => {
                self.export_save_dialog_rx = None;
                self.export_save_dialog_pending = false;

                let Some(request) = self.pending_export_request.take() else {
                    return;
                };

                if result.request_id != request.request_id {
                    return;
                }

                if let Some(path) = result.path {
                    self.start_export_worker(request, path);
                }
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                self.export_save_dialog_rx = None;
                self.export_save_dialog_pending = false;
                self.pending_export_request = None;
                self.last_error =
                    Some("Export destination chooser failed before returning a path.".to_owned());
            }
        }
    }

    fn start_export_worker(&mut self, request: PendingExportRequest, path: PathBuf) {
        let (sender, receiver) = mpsc::channel();
        let request_id = request.request_id;

        self.export_rx = Some(receiver);
        self.export_pending = true;
        self.last_error = None;
        self.last_export_message = None;

        thread::spawn(move || {
            let result = match request.format {
                ExportFormat::FlattenedBits => export_flattened_bits(&request.view, &path),
                ExportFormat::Pcap => export_pcap(&request.view, &path, &request.pcap),
                ExportFormat::Wav => export_wav(&request.view, &path, &request.wav),
            };

            let _ = sender.send(ExportWorkerResult {
                request_id,
                format: request.format,
                path,
                result,
            });
        });
    }

    fn poll_export(&mut self) {
        let Some(receiver) = &self.export_rx else {
            return;
        };

        match receiver.try_recv() {
            Ok(result) => {
                if result.request_id != self.export_request_id {
                    return;
                }

                self.export_rx = None;
                self.export_pending = false;
                match result.result {
                    Ok(()) => {
                        self.last_error = None;
                        self.last_export_message = Some(format!(
                            "{} written to {}",
                            result.format.success_label(),
                            result.path.display()
                        ));
                    }
                    Err(error) => {
                        self.last_export_message = None;
                        self.last_error = Some(error);
                    }
                }
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                self.export_rx = None;
                self.export_pending = false;
                self.last_export_message = None;
                self.last_error = Some("Export worker stopped unexpectedly.".to_owned());
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
                        self.schedule_autocorrelation();
                        self.last_error = None;
                    }
                    Err(error) => {
                        self.derived_view = None;
                        self.invalidate_render_caches();
                        self.clear_autocorrelation_state();
                        self.last_error = Some(error);
                    }
                }
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                self.rebuild_rx = None;
                self.rebuild_pending = false;
                self.clear_autocorrelation_state();
                self.last_error = Some("Filtered view worker stopped unexpectedly.".to_owned());
            }
        }
    }

    fn poll_autocorrelation(&mut self) {
        let Some(receiver) = &self.autocorrelation_rx else {
            return;
        };

        match receiver.try_recv() {
            Ok(worker_result) => {
                if worker_result.request_id != self.autocorrelation_request_id
                    || worker_result.view_revision != self.derived_view_revision
                {
                    return;
                }

                self.autocorrelation_rx = None;
                self.autocorrelation_pending = false;
                self.autocorrelation_progress = None;
                self.autocorrelation_progress_total = 0;
                self.autocorrelation_result = Some(worker_result.result);
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                self.autocorrelation_rx = None;
                self.autocorrelation_pending = false;
                self.autocorrelation_progress = None;
                self.autocorrelation_progress_total = 0;
            }
        }
    }

    fn schedule_rebuild(&mut self) {
        let Some(document) = &self.document else {
            self.derived_view = None;
            self.clear_autocorrelation_state();
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
        self.clear_autocorrelation_state();
        self.clear_drawn_columns();
        self.pending_bit_scroll_to_row = Some(0);
        self.pending_text_scroll_to_row = Some(0);
        self.current_bit_scroll_row = 0;
        self.current_text_scroll_row = 0;
        self.last_error = None;
        self.last_export_message = None;

        thread::spawn(move || {
            let result = BinaryDocument::open(&path)
                .and_then(|document| document.build_derived_view(&pipeline));
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
            self.trim_drawn_columns_to_row_width();
            needs_repaint = true;
        } else if self.row_width_bits > self.target_row_width_bits {
            self.row_width_bits = self
                .row_width_bits
                .saturating_sub(ROW_WIDTH_STEP_BITS)
                .max(self.target_row_width_bits);
            self.invalidate_render_caches();
            self.trim_drawn_columns_to_row_width();
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
        self.target_row_width_bits = self
            .target_row_width_bits
            .saturating_add(ROW_WIDTH_STEP_BITS);
        self.sync_row_width_input();
    }

    fn decrease_row_width(&mut self) {
        self.target_row_width_bits = self
            .target_row_width_bits
            .saturating_sub(ROW_WIDTH_STEP_BITS)
            .max(MIN_ROW_WIDTH_BITS);
        self.sync_row_width_input();
    }

    fn increase_bit_size(&mut self) {
        self.target_bit_size = (self.target_bit_size + BIT_SIZE_STEP).min(MAX_BIT_SIZE);
    }

    fn decrease_bit_size(&mut self) {
        self.target_bit_size = (self.target_bit_size - BIT_SIZE_STEP).max(MIN_BIT_SIZE);
    }

    fn apply_row_width_input(&mut self, commit: bool) {
        let trimmed = self.row_width_input.trim();
        if trimmed.is_empty() {
            if commit {
                self.sync_row_width_input();
            }
            return;
        }

        let Ok(parsed) = trimmed.parse::<usize>() else {
            if commit {
                self.sync_row_width_input();
            }
            return;
        };

        let clamped = parsed.clamp(MIN_ROW_WIDTH_BITS, MAX_ROW_WIDTH_BITS);
        self.set_row_width_immediately(clamped);

        if commit || parsed != clamped {
            self.sync_row_width_input();
        }
    }

    fn apply_autocorrelation_max_width_input(&mut self, commit: bool) {
        let trimmed = self.autocorrelation_max_width_input.trim();
        if trimmed.is_empty() {
            if commit {
                self.sync_autocorrelation_max_width_input();
            }
            return;
        }

        let Ok(parsed) = trimmed.parse::<usize>() else {
            if commit {
                self.sync_autocorrelation_max_width_input();
            }
            return;
        };

        let clamped = parsed.clamp(
            MIN_AUTOCORRELATION_MAX_WIDTH_BITS,
            MAX_AUTOCORRELATION_MAX_WIDTH_BITS,
        );
        self.set_autocorrelation_max_width(clamped);

        if commit || parsed != clamped {
            self.sync_autocorrelation_max_width_input();
        }
    }

    fn apply_autocorrelation_sample_bytes_input(&mut self, commit: bool) {
        let trimmed = self.autocorrelation_sample_bytes_input.trim();
        if trimmed.is_empty() {
            if commit {
                self.sync_autocorrelation_sample_bytes_input();
            }
            return;
        }

        let Ok(parsed) = trimmed.parse::<usize>() else {
            if commit {
                self.sync_autocorrelation_sample_bytes_input();
            }
            return;
        };

        let clamped = parsed.clamp(
            MIN_AUTOCORRELATION_SAMPLE_BYTES,
            MAX_AUTOCORRELATION_SAMPLE_BYTES,
        );
        self.set_autocorrelation_sample_bytes(clamped);

        if commit || parsed != clamped {
            self.sync_autocorrelation_sample_bytes_input();
        }
    }

    fn set_row_width_immediately(&mut self, row_width_bits: usize) {
        let clamped = row_width_bits.clamp(MIN_ROW_WIDTH_BITS, MAX_ROW_WIDTH_BITS);
        if self.target_row_width_bits != clamped || self.row_width_bits != clamped {
            self.target_row_width_bits = clamped;
            self.row_width_bits = clamped;
            self.invalidate_render_caches();
            self.trim_drawn_columns_to_row_width();
        }
        self.row_width_input = clamped.to_string();
    }

    fn set_autocorrelation_max_width(&mut self, max_width_bits: usize) {
        let clamped = max_width_bits.clamp(
            MIN_AUTOCORRELATION_MAX_WIDTH_BITS,
            MAX_AUTOCORRELATION_MAX_WIDTH_BITS,
        );

        if self.autocorrelation_max_width_bits != clamped {
            self.autocorrelation_max_width_bits = clamped;
            self.sync_autocorrelation_max_width_input();
            self.schedule_autocorrelation();
        } else {
            self.sync_autocorrelation_max_width_input();
        }
    }

    fn set_autocorrelation_sample_bytes(&mut self, sample_bytes: usize) {
        let clamped = sample_bytes.clamp(
            MIN_AUTOCORRELATION_SAMPLE_BYTES,
            MAX_AUTOCORRELATION_SAMPLE_BYTES,
        );

        if self.autocorrelation_sample_bytes != clamped {
            self.autocorrelation_sample_bytes = clamped;
            self.sync_autocorrelation_sample_bytes_input();
            self.schedule_autocorrelation();
        } else {
            self.sync_autocorrelation_sample_bytes_input();
        }
    }

    fn sync_row_width_input(&mut self) {
        self.row_width_input = self.target_row_width_bits.to_string();
    }

    fn sync_autocorrelation_max_width_input(&mut self) {
        self.autocorrelation_max_width_input = self.autocorrelation_max_width_bits.to_string();
    }

    fn sync_autocorrelation_sample_bytes_input(&mut self) {
        self.autocorrelation_sample_bytes_input = self.autocorrelation_sample_bytes.to_string();
    }
}

fn paint_autocorrelation_graph(
    ui: &mut Ui,
    result: &AutoCorrelationResult,
    current_width_bits: usize,
) -> Option<usize> {
    let available_max_width_bits = result.available_max_width_bits();
    if available_max_width_bits == 0 {
        return None;
    }

    let desired_size = Vec2::new(
        ui.available_width().clamp(
            AUTOCORRELATION_GRAPH_MIN_WIDTH,
            AUTOCORRELATION_GRAPH_MAX_WIDTH,
        ),
        AUTOCORRELATION_GRAPH_HEIGHT,
    );
    let (rect, response) = ui.allocate_exact_size(desired_size, egui::Sense::click());
    let plot_rect = Rect::from_min_max(
        egui::pos2(rect.left() + 14.0, rect.top() + 8.0),
        egui::pos2(rect.right() - 14.0, rect.bottom() - 22.0),
    );
    let painter = ui.painter();
    let label_font = FontId::new(11.0, FontFamily::Monospace);
    let baseline_y = autocorrelation_graph_y(plot_rect, 0.0);

    painter.line_segment(
        [
            egui::pos2(plot_rect.left(), baseline_y),
            egui::pos2(plot_rect.right(), baseline_y),
        ],
        Stroke::new(1.0, ACCENT_SOFT),
    );

    painter.text(
        egui::pos2(plot_rect.left(), rect.bottom() - 4.0),
        egui::Align2::LEFT_BOTTOM,
        "1",
        label_font.clone(),
        TEXT_MUTED,
    );
    painter.text(
        egui::pos2(plot_rect.right(), rect.bottom() - 4.0),
        egui::Align2::RIGHT_BOTTOM,
        available_max_width_bits.to_string(),
        label_font.clone(),
        TEXT_MUTED,
    );
    painter.text(
        egui::pos2(rect.left(), plot_rect.top()),
        egui::Align2::LEFT_TOP,
        "+1.0",
        label_font.clone(),
        TEXT_MUTED,
    );
    painter.text(
        egui::pos2(rect.left(), baseline_y),
        egui::Align2::LEFT_CENTER,
        "0",
        label_font.clone(),
        TEXT_MUTED,
    );
    painter.text(
        egui::pos2(rect.left(), plot_rect.bottom()),
        egui::Align2::LEFT_BOTTOM,
        "-1.0",
        label_font.clone(),
        TEXT_MUTED,
    );

    let mut points = Vec::with_capacity(result.samples.len());
    for sample in &result.samples {
        points.push(egui::pos2(
            autocorrelation_graph_x(plot_rect, sample.width_bits, available_max_width_bits),
            autocorrelation_graph_y(plot_rect, sample.score),
        ));
    }

    if points.len() > 1 {
        painter.add(egui::Shape::line(points, Stroke::new(2.0, ACCENT_COLOR)));
    } else if let Some(point) = points.first().copied() {
        painter.circle_filled(point, 3.0, ACCENT_COLOR);
    }

    if let Some(best_width_bits) = result.best_width_bits
        && let Some(sample) = result.sample_for_width(best_width_bits)
    {
        let point = egui::pos2(
            autocorrelation_graph_x(plot_rect, best_width_bits, available_max_width_bits),
            autocorrelation_graph_y(plot_rect, sample.score),
        );
        painter.line_segment(
            [
                egui::pos2(point.x, plot_rect.top()),
                egui::pos2(point.x, plot_rect.bottom()),
            ],
            Stroke::new(1.0, ACCENT_SOFT),
        );
        painter.circle_filled(point, 4.0, ACCENT_COLOR);
    }

    if let Some(sample) = result.sample_for_width(current_width_bits) {
        let point = egui::pos2(
            autocorrelation_graph_x(plot_rect, current_width_bits, available_max_width_bits),
            autocorrelation_graph_y(plot_rect, sample.score),
        );
        painter.line_segment(
            [
                egui::pos2(point.x, plot_rect.top()),
                egui::pos2(point.x, plot_rect.bottom()),
            ],
            Stroke::new(1.0, BYTE_DIVIDER_COLOR),
        );
        painter.circle_filled(point, 4.0, BYTE_DIVIDER_COLOR);
    }

    let hovered_width_bits = response.hover_pos().and_then(|pointer| {
        autocorrelation_graph_width(plot_rect, pointer.x, available_max_width_bits)
    });

    if let Some(width_bits) = hovered_width_bits
        && let Some(sample) = result.sample_for_width(width_bits)
    {
        let point = egui::pos2(
            autocorrelation_graph_x(plot_rect, width_bits, available_max_width_bits),
            autocorrelation_graph_y(plot_rect, sample.score),
        );
        painter.line_segment(
            [
                egui::pos2(point.x, plot_rect.top()),
                egui::pos2(point.x, plot_rect.bottom()),
            ],
            Stroke::new(1.0, TEXT_PRIMARY),
        );
        painter.circle_filled(point, 4.0, TEXT_PRIMARY);
        painter.text(
            egui::pos2(rect.right() - 4.0, rect.top() + 2.0),
            egui::Align2::RIGHT_TOP,
            format!(
                "{width_bits} bits  {:.3}  {} cmp",
                sample.score, sample.comparisons
            ),
            label_font,
            TEXT_PRIMARY,
        );
    }

    if response.clicked() {
        return hovered_width_bits;
    }

    None
}

fn autocorrelation_graph_x(plot_rect: Rect, width_bits: usize, max_width_bits: usize) -> f32 {
    if max_width_bits <= 1 {
        return plot_rect.center().x;
    }

    let fraction = (width_bits.saturating_sub(1)) as f32 / (max_width_bits - 1) as f32;
    plot_rect.left() + fraction * plot_rect.width()
}

fn autocorrelation_graph_y(plot_rect: Rect, score: f32) -> f32 {
    let normalized = ((score.clamp(-1.0, 1.0) + 1.0) * 0.5).clamp(0.0, 1.0);
    plot_rect.bottom() - normalized * plot_rect.height()
}

fn autocorrelation_graph_width(plot_rect: Rect, x: f32, max_width_bits: usize) -> Option<usize> {
    if max_width_bits == 0 {
        return None;
    }

    if max_width_bits == 1 {
        return Some(1);
    }

    let fraction = ((x - plot_rect.left()) / plot_rect.width()).clamp(0.0, 1.0);
    Some(1 + (fraction * (max_width_bits - 1) as f32).round() as usize)
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TextPaneKind {
    Hex,
    Ascii,
}

fn paint_column_drag_overlay(
    ui: &mut Ui,
    rect: Rect,
    bit_size: f32,
    row_width_bits: usize,
    highlighted_bit_ranges: &[(usize, usize)],
) {
    if row_width_bits == 0 || highlighted_bit_ranges.is_empty() {
        return;
    }

    for (start_bit, end_bit) in highlighted_bit_ranges.iter().copied() {
        let start_bit = start_bit.min(row_width_bits.saturating_sub(1));
        let end_bit = end_bit.min(row_width_bits);
        if start_bit >= end_bit {
            continue;
        }

        let highlight_rect = Rect::from_min_max(
            egui::pos2(rect.left() + start_bit as f32 * bit_size, rect.top()),
            egui::pos2(rect.left() + end_bit as f32 * bit_size, rect.bottom()),
        );
        ui.painter().rect_filled(
            highlight_rect,
            CornerRadius::ZERO,
            ACCENT_SOFT.linear_multiply(0.35),
        );
    }
}

fn pointer_bit_col_in_bit_grid(
    rect: Rect,
    horizontal_scroll: f32,
    bit_size: f32,
    row_width_bits: usize,
    pointer_pos: Option<egui::Pos2>,
) -> Option<usize> {
    if row_width_bits == 0 {
        return None;
    }

    let pointer_pos = pointer_pos?;
    let bit_col = ((((pointer_pos.x - rect.left()) + horizontal_scroll) / bit_size)
        .floor()
        .max(0.0) as usize)
        .min(row_width_bits.saturating_sub(1));
    Some(bit_col)
}

fn pointer_byte_col_in_text_pane(
    rect: Rect,
    horizontal_scroll: f32,
    bytes_per_row: usize,
    pane_kind: TextPaneKind,
    text_char_width: f32,
    pointer_pos: Option<egui::Pos2>,
) -> Option<usize> {
    if bytes_per_row == 0 {
        return None;
    }

    let pointer_pos = pointer_pos?;
    let cell_width = text_pane_column_step(pane_kind, text_char_width);
    let local_x =
        ((pointer_pos.x - rect.left()) + horizontal_scroll - TEXT_CELL_PADDING_X).max(0.0);
    Some(((local_x / cell_width).floor().max(0.0) as usize).min(bytes_per_row.saturating_sub(1)))
}

fn paint_single_text_row(
    ui: &mut Ui,
    text: &str,
    row_height: f32,
    width: f32,
    color: Color32,
    row_index: usize,
    pane_kind: TextPaneKind,
    text_char_width: f32,
    highlighted_byte_ranges: &[(usize, usize)],
) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, row_height), egui::Sense::hover());
    let row_fill = if row_index % 2 == 0 {
        SURFACE_BG
    } else {
        SURFACE_ALT_BG
    };
    ui.painter().rect_filled(rect, CornerRadius::ZERO, row_fill);
    paint_text_column_overlay(
        ui,
        rect,
        pane_kind,
        text_char_width,
        highlighted_byte_ranges,
    );
    ui.painter().text(
        egui::pos2(rect.left() + TEXT_CELL_PADDING_X, rect.center().y),
        egui::Align2::LEFT_CENTER,
        text,
        text_pane_font_id(),
        color,
    );
}

fn paint_text_column_overlay(
    ui: &mut Ui,
    rect: Rect,
    pane_kind: TextPaneKind,
    text_char_width: f32,
    highlighted_byte_ranges: &[(usize, usize)],
) {
    if highlighted_byte_ranges.is_empty() {
        return;
    }

    let x_step = text_pane_column_step(pane_kind, text_char_width);
    for (start_col, end_col) in highlighted_byte_ranges.iter().copied() {
        let highlight_rect = Rect::from_min_max(
            egui::pos2(
                rect.left() + TEXT_CELL_PADDING_X + start_col as f32 * x_step,
                rect.top(),
            ),
            egui::pos2(
                rect.left() + TEXT_CELL_PADDING_X + end_col as f32 * x_step,
                rect.bottom(),
            ),
        );
        ui.painter().rect_filled(
            highlight_rect,
            CornerRadius::ZERO,
            ACCENT_SOFT.linear_multiply(0.45),
        );
    }
}

fn text_pane_font_id() -> FontId {
    FontId::new(TEXT_FONT_SIZE, FontFamily::Monospace)
}

fn text_pane_char_width(ui: &Ui) -> f32 {
    let font_id = text_pane_font_id();
    ui.fonts_mut(|fonts| fonts.glyph_width(&font_id, '0'))
        .max(1.0)
}

fn text_pane_content_width(
    bytes_per_row: usize,
    pane_kind: TextPaneKind,
    text_char_width: f32,
) -> f32 {
    text_pane_column_step(pane_kind, text_char_width) * bytes_per_row as f32
        + TEXT_CELL_PADDING_X * 2.0
}

fn text_pane_column_step(pane_kind: TextPaneKind, text_char_width: f32) -> f32 {
    match pane_kind {
        TextPaneKind::Hex => text_char_width * 3.0,
        TextPaneKind::Ascii => text_char_width,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BitViewerApp, DrawGranularity, TextPaneKind, pointer_bit_col_in_bit_grid,
        pointer_byte_col_in_text_pane,
    };
    use eframe::egui::{Rect, pos2};

    #[test]
    fn byte_draw_segments_toggle_full_bytes() {
        let mut app = BitViewerApp::default();
        app.row_width_bits = 16;

        app.apply_draw_segment(DrawGranularity::Byte, 0, 1, super::DrawStrokeMode::Paint);

        assert_eq!(app.highlighted_bit_ranges(), vec![(0, 16)]);

        app.apply_draw_segment(DrawGranularity::Byte, 1, 1, super::DrawStrokeMode::Erase);

        assert_eq!(app.highlighted_bit_ranges(), vec![(0, 8)]);
    }

    #[test]
    fn bit_grid_pointer_maps_to_bit_columns() {
        let rect = Rect::from_min_max(pos2(10.0, 20.0), pos2(110.0, 220.0));

        assert_eq!(
            pointer_bit_col_in_bit_grid(rect, 0.0, 5.0, 32, Some(pos2(10.0, 40.0))),
            Some(0)
        );
        assert_eq!(
            pointer_bit_col_in_bit_grid(rect, 20.0, 5.0, 32, Some(pos2(30.0, 40.0))),
            Some(8)
        );
        assert_eq!(
            pointer_bit_col_in_bit_grid(rect, 500.0, 5.0, 32, Some(pos2(109.0, 40.0))),
            Some(31)
        );
    }

    #[test]
    fn text_pointer_maps_to_byte_columns() {
        let rect = Rect::from_min_max(pos2(10.0, 20.0), pos2(210.0, 220.0));
        let text_char_width = 8.0;

        assert_eq!(
            pointer_byte_col_in_text_pane(
                rect,
                0.0,
                16,
                TextPaneKind::Ascii,
                text_char_width,
                Some(pos2(19.0, 40.0))
            ),
            Some(0)
        );
        assert_eq!(
            pointer_byte_col_in_text_pane(
                rect,
                0.0,
                16,
                TextPaneKind::Hex,
                text_char_width,
                Some(pos2(45.0, 40.0))
            ),
            Some(1)
        );
        assert_eq!(
            pointer_byte_col_in_text_pane(
                rect,
                999.0,
                16,
                TextPaneKind::Ascii,
                text_char_width,
                Some(pos2(209.0, 40.0))
            ),
            Some(15)
        );
    }

    #[test]
    fn row_width_input_accepts_one_bit() {
        let mut app = BitViewerApp::default();
        app.row_width_input = "1".to_owned();

        app.apply_row_width_input(true);

        assert_eq!(app.row_width_bits, 1);
        assert_eq!(app.target_row_width_bits, 1);
        assert_eq!(app.row_width_input, "1");
    }
}
