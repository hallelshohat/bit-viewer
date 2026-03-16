use crate::filters::DerivedView;

pub const ASCII_PLACEHOLDER: char = '.';
pub const BIT_VALUE_NO_DATA: u8 = 2;

#[derive(Clone, Debug)]
pub struct RowData {
    pub hex: String,
    pub ascii: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HoverPosition {
    pub row_index: usize,
    pub column_index: usize,
    pub bit_offset: usize,
    pub byte_offset: usize,
}

#[derive(Clone, Debug)]
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

    let (hex, ascii) = render_text_columns(group, start_bit, bits_to_take);
    RowData { hex, ascii }
}

pub fn build_bit_window(
    view: &DerivedView,
    layout: &RowLayout,
    start_row: usize,
    row_count: usize,
    start_col: usize,
    col_count: usize,
) -> Vec<u8> {
    let mut bitmap = vec![BIT_VALUE_NO_DATA; row_count.saturating_mul(col_count)];

    if col_count == 0 {
        return bitmap;
    }

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
        if start_col >= bits_to_take {
            continue;
        }

        let row_start = row_offset.saturating_mul(col_count);
        let visible_bits = bits_to_take.saturating_sub(start_col).min(col_count);

        for bit_offset in 0..visible_bits {
            bitmap[row_start + bit_offset] =
                group.bit(start_bit + start_col + bit_offset).unwrap_or(0);
        }
    }

    bitmap
}

pub fn bit_hover_position(
    view: &DerivedView,
    layout: &RowLayout,
    row_index: usize,
    bit_col: usize,
) -> Option<HoverPosition> {
    let RowSlice {
        group_index,
        start_bit,
        bits_to_take,
    } = row_slice(view, layout, row_index)?;
    if bit_col >= bits_to_take {
        return None;
    }

    let bit_offset = view.group_prefix_bits()[group_index] + start_bit + bit_col;
    Some(HoverPosition {
        row_index,
        column_index: bit_col,
        bit_offset,
        byte_offset: bit_offset / 8,
    })
}

pub fn byte_hover_position(
    view: &DerivedView,
    layout: &RowLayout,
    row_index: usize,
    byte_col: usize,
) -> Option<HoverPosition> {
    let RowSlice {
        group_index,
        start_bit,
        bits_to_take,
    } = row_slice(view, layout, row_index)?;
    let bit_col = byte_col.saturating_mul(8);
    if bit_col >= bits_to_take {
        return None;
    }

    let bit_offset = view.group_prefix_bits()[group_index] + start_bit + bit_col;
    Some(HoverPosition {
        row_index,
        column_index: byte_col,
        bit_offset,
        byte_offset: bit_offset / 8,
    })
}

struct RowSlice {
    group_index: usize,
    start_bit: usize,
    bits_to_take: usize,
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

fn row_slice(view: &DerivedView, layout: &RowLayout, row_index: usize) -> Option<RowSlice> {
    let (group_index, row_in_group) = locate_group_row(layout, row_index)?;
    let group = &view.groups()[group_index];
    let start_bit = row_in_group.saturating_mul(layout.row_width_bits);
    let bits_to_take = group
        .len_bits()
        .saturating_sub(start_bit)
        .min(layout.row_width_bits);
    Some(RowSlice {
        group_index,
        start_bit,
        bits_to_take,
    })
}

fn render_text_columns(
    group: &crate::filters::DerivedGroup,
    start_bit: usize,
    bit_len: usize,
) -> (String, String) {
    let byte_count = bit_len.div_ceil(8);
    let mut hex = String::with_capacity(byte_count.saturating_mul(3).saturating_sub(1));
    let mut ascii = String::with_capacity(byte_count);

    for byte_index in 0..byte_count {
        let mut byte = 0u8;
        let chunk_start = start_bit + byte_index.saturating_mul(8);
        let chunk_len = bit_len.saturating_sub(byte_index.saturating_mul(8)).min(8);

        for bit_index in 0..chunk_len {
            byte |= (group.bit(chunk_start + bit_index).unwrap_or(0) & 1) << (7 - bit_index);
        }

        if byte_index > 0 {
            hex.push(' ');
        }
        push_hex_byte(&mut hex, byte);

        let character = if byte.is_ascii_graphic() || byte == b' ' {
            char::from(byte)
        } else {
            ASCII_PLACEHOLDER
        };
        ascii.push(character);
    }

    (hex, ascii)
}

fn push_hex_byte(output: &mut String, byte: u8) {
    const HEX_DIGITS: &[u8; 16] = b"0123456789ABCDEF";

    output.push(HEX_DIGITS[(byte >> 4) as usize] as char);
    output.push(HEX_DIGITS[(byte & 0x0F) as usize] as char);
}

#[cfg(test)]
mod tests {
    use crate::filters::build_derived_view;

    use super::{
        BIT_VALUE_NO_DATA, HoverPosition, bit_hover_position, build_bit_window, build_row_layout,
        byte_hover_position,
    };

    #[test]
    fn bit_window_marks_cells_past_row_data_as_no_data() {
        let view = build_derived_view(&[0b1010_0000], &Default::default()).unwrap();
        let layout = build_row_layout(&view, 12);

        let bitmap = build_bit_window(&view, &layout, 0, 1, 0, 12);

        assert_eq!(&bitmap[..8], &[1, 0, 1, 0, 0, 0, 0, 0]);
        assert_eq!(&bitmap[8..], &[BIT_VALUE_NO_DATA; 4]);
    }

    #[test]
    fn bit_hover_position_uses_absolute_bit_offsets() {
        let view = build_derived_view(&[0xAB, 0xCD], &Default::default()).unwrap();
        let layout = build_row_layout(&view, 8);

        assert_eq!(
            bit_hover_position(&view, &layout, 1, 3),
            Some(HoverPosition {
                row_index: 1,
                column_index: 3,
                bit_offset: 11,
                byte_offset: 1,
            })
        );
    }

    #[test]
    fn byte_hover_position_rejects_partial_row_columns_past_data() {
        let view = build_derived_view(&[0xAA, 0xB0], &Default::default()).unwrap();
        let layout = build_row_layout(&view, 12);

        assert_eq!(
            byte_hover_position(&view, &layout, 0, 0).map(|pos| pos.bit_offset),
            Some(0)
        );
        assert_eq!(
            byte_hover_position(&view, &layout, 0, 1).map(|pos| pos.bit_offset),
            Some(8)
        );
        assert_eq!(byte_hover_position(&view, &layout, 0, 2), None);
    }
}
