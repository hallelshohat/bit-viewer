use crate::filters::DerivedView;

pub const ASCII_PLACEHOLDER: char = '.';

pub struct RowData {
    pub hex: String,
    pub ascii: String,
}

pub struct RowLayout {
    pub row_width_bits: usize,
    group_row_offsets: Vec<usize>,
    total_rows: usize,
}

impl RowLayout {
    pub fn total_rows(&self) -> usize {
        self.total_rows
    }
}

pub fn build_row_layout(view: &DerivedView, row_width_bits: usize) -> RowLayout {
    if row_width_bits == 0 {
        return RowLayout {
            row_width_bits,
            group_row_offsets: vec![0; view.group_count() + 1],
            total_rows: 0,
        };
    }

    let mut group_row_offsets = Vec::with_capacity(view.group_count() + 1);
    let mut total_rows = 0usize;
    group_row_offsets.push(0);

    for group in view.groups() {
        total_rows = total_rows.saturating_add(group.len_bits().div_ceil(row_width_bits));
        group_row_offsets.push(total_rows);
    }

    RowLayout {
        row_width_bits,
        group_row_offsets,
        total_rows,
    }
}

pub fn bit_offset_to_row(view: &DerivedView, layout: &RowLayout, bit_offset: usize) -> usize {
    if layout.row_width_bits == 0 || layout.total_rows == 0 || view.total_bits() == 0 {
        return 0;
    }

    let clamped = bit_offset.min(view.total_bits().saturating_sub(1));
    let group_index = view
        .group_prefix_bits()
        .partition_point(|group_start| *group_start <= clamped)
        .saturating_sub(1)
        .min(view.group_count().saturating_sub(1));
    let group_start_bit = view.group_prefix_bits()[group_index];
    let group_relative_bit = clamped.saturating_sub(group_start_bit);

    layout.group_row_offsets[group_index] + (group_relative_bit / layout.row_width_bits)
}

pub fn build_row(view: &DerivedView, layout: &RowLayout, row_index: usize) -> RowData {
    let Some((group_index, row_in_group)) = locate_group_row(layout, row_index) else {
        return RowData {
            hex: String::new(),
            ascii: String::new(),
        };
    };

    let group = &view.groups()[group_index];
    let start_bit = row_in_group.saturating_mul(layout.row_width_bits);
    let bits_to_take = group
        .len_bits()
        .saturating_sub(start_bit)
        .min(layout.row_width_bits);

    let mut bits = Vec::with_capacity(bits_to_take);
    for bit_offset in start_bit..start_bit + bits_to_take {
        bits.push(group.bit(bit_offset).unwrap_or(0));
    }

    let (hex, ascii) = render_text_columns(&bits);
    RowData { hex, ascii }
}

pub fn build_bit_rows(
    view: &DerivedView,
    layout: &RowLayout,
    start_row: usize,
    row_count: usize,
) -> Vec<u8> {
    let mut bitmap = vec![0; row_count.saturating_mul(layout.row_width_bits)];

    for row_offset in 0..row_count {
        let row_index = start_row + row_offset;
        let Some((group_index, row_in_group)) = locate_group_row(layout, row_index) else {
            continue;
        };

        let group = &view.groups()[group_index];
        let start_bit = row_in_group.saturating_mul(layout.row_width_bits);
        let bits_to_take = group
            .len_bits()
            .saturating_sub(start_bit)
            .min(layout.row_width_bits);
        let row_start = row_offset.saturating_mul(layout.row_width_bits);

        for bit_offset in 0..bits_to_take {
            bitmap[row_start + bit_offset] = group.bit(start_bit + bit_offset).unwrap_or(0);
        }
    }

    bitmap
}

fn locate_group_row(layout: &RowLayout, row_index: usize) -> Option<(usize, usize)> {
    if layout.total_rows == 0 || row_index >= layout.total_rows {
        return None;
    }

    let group_index = layout
        .group_row_offsets
        .partition_point(|offset| *offset <= row_index)
        .saturating_sub(1);
    let row_start = layout.group_row_offsets[group_index];
    Some((group_index, row_index.saturating_sub(row_start)))
}

fn render_text_columns(bits: &[u8]) -> (String, String) {
    let mut hex = String::new();
    let mut ascii = String::new();

    for chunk in bits.chunks(8) {
        let mut byte = 0u8;
        for (index, bit) in chunk.iter().enumerate() {
            byte |= (bit & 1) << (7 - index);
        }

        if !hex.is_empty() {
            hex.push(' ');
        }
        hex.push_str(&format!("{byte:02X}"));

        let character = if byte.is_ascii_graphic() || byte == b' ' {
            char::from(byte)
        } else {
            ASCII_PLACEHOLDER
        };
        ascii.push(character);
    }

    (hex, ascii)
}
