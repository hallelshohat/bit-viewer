#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FilterPipeline {
    pub steps: Vec<FilterStep>,
}

impl FilterPipeline {
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FilterStep {
    SyncOnPreamble {
        bits: String,
    },
    ReverseBitsPerByte,
    InvertBits,
    XorMask {
        mask: u8,
    },
    Flatten,
    KeepGroupsLongerThanBytes {
        min_bytes: usize,
    },
    SelectBitRangeFromGroup {
        start_bit: usize,
        length_bits: usize,
    },
}

impl FilterStep {
    pub fn label(&self) -> &'static str {
        match self {
            Self::SyncOnPreamble { .. } => "Sync on preamble",
            Self::ReverseBitsPerByte => "Reverse bits in each byte",
            Self::InvertBits => "Invert bits",
            Self::XorMask { .. } => "XOR byte mask",
            Self::Flatten => "Flatten groups",
            Self::KeepGroupsLongerThanBytes { .. } => "Keep groups longer than bytes",
            Self::SelectBitRangeFromGroup { .. } => "Select bit range from group",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DerivedGroup {
    data: BitBuffer,
}

impl DerivedGroup {
    pub fn len_bits(&self) -> usize {
        self.data.len_bits()
    }

    pub fn len_bytes_rounded_up(&self) -> usize {
        self.data.len_bytes_rounded_up()
    }

    pub fn bit(&self, index: usize) -> Option<u8> {
        self.data.bit(index)
    }

    pub fn packed_bytes(&self) -> &[u8] {
        self.data.bytes()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DerivedView {
    groups: Vec<DerivedGroup>,
    group_prefix_bits: Vec<usize>,
    total_bits: usize,
}

impl DerivedView {
    pub fn new(groups: Vec<DerivedGroup>) -> Self {
        let mut group_prefix_bits = Vec::with_capacity(groups.len() + 1);
        group_prefix_bits.push(0);

        let mut total_bits = 0usize;
        for group in &groups {
            total_bits = total_bits.saturating_add(group.len_bits());
            group_prefix_bits.push(total_bits);
        }

        Self {
            groups,
            group_prefix_bits,
            total_bits,
        }
    }

    pub fn groups(&self) -> &[DerivedGroup] {
        &self.groups
    }

    pub fn group_count(&self) -> usize {
        self.groups.len()
    }

    pub fn total_bits(&self) -> usize {
        self.total_bits
    }

    pub fn total_bytes_rounded_up(&self) -> usize {
        self.total_bits.div_ceil(8)
    }

    pub fn group_prefix_bits(&self) -> &[usize] {
        &self.group_prefix_bits
    }

    pub fn flattened_packed_bytes(&self) -> Vec<u8> {
        let buffers = self
            .groups
            .iter()
            .map(|group| group.data.clone())
            .collect::<Vec<_>>();
        BitBuffer::concatenate(&buffers).into_bytes()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct BitBuffer {
    bytes: Vec<u8>,
    bit_len: usize,
}

impl BitBuffer {
    fn from_bytes(bytes: Vec<u8>) -> Self {
        let bit_len = bytes.len().saturating_mul(8);
        Self { bytes, bit_len }
    }

    fn len_bits(&self) -> usize {
        self.bit_len
    }

    fn len_bytes_rounded_up(&self) -> usize {
        self.bit_len.div_ceil(8)
    }

    fn is_empty(&self) -> bool {
        self.bit_len == 0
    }

    fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    fn bit(&self, index: usize) -> Option<u8> {
        if index >= self.bit_len {
            return None;
        }

        let byte = *self.bytes.get(index / 8)?;
        Some((byte >> (7 - (index % 8))) & 1)
    }

    fn push_bit(&mut self, bit: u8) {
        if self.bit_len % 8 == 0 {
            self.bytes.push(0);
        }

        if bit != 0 {
            let last_index = self.bytes.len() - 1;
            let shift = 7 - (self.bit_len % 8);
            self.bytes[last_index] |= 1 << shift;
        }

        self.bit_len += 1;
    }

    fn slice_bits(&self, start_bit: usize, length_bits: usize) -> Self {
        if start_bit >= self.bit_len || length_bits == 0 {
            return Self::default();
        }

        let end_bit = start_bit.saturating_add(length_bits).min(self.bit_len);
        let mut sliced = Self::default();
        for bit_index in start_bit..end_bit {
            sliced.push_bit(self.bit(bit_index).unwrap_or(0));
        }
        sliced
    }

    fn map_bytes(&self, mut transform: impl FnMut(u8) -> u8) -> Self {
        let mut bytes = self
            .bytes
            .iter()
            .copied()
            .map(&mut transform)
            .collect::<Vec<_>>();
        mask_unused_tail_bits(&mut bytes, self.bit_len);
        Self {
            bytes,
            bit_len: self.bit_len,
        }
    }

    fn concatenate(buffers: &[Self]) -> Self {
        let mut combined = Self::default();
        for buffer in buffers {
            for bit_index in 0..buffer.len_bits() {
                combined.push_bit(buffer.bit(bit_index).unwrap_or(0));
            }
        }
        combined
    }
}

enum PipelineState {
    Flat(BitBuffer),
    Grouped(Vec<BitBuffer>),
}

impl PipelineState {
    fn into_flat(self) -> BitBuffer {
        match self {
            Self::Flat(buffer) => buffer,
            Self::Grouped(groups) => BitBuffer::concatenate(&groups),
        }
    }

    fn map_bytes(self, transform: impl FnMut(u8) -> u8 + Copy) -> Self {
        match self {
            Self::Flat(buffer) => Self::Flat(buffer.map_bytes(transform)),
            Self::Grouped(groups) => Self::Grouped(
                groups
                    .into_iter()
                    .map(|group| group.map_bytes(transform))
                    .collect(),
            ),
        }
    }
}

pub fn build_derived_view(bytes: &[u8], pipeline: &FilterPipeline) -> Result<DerivedView, String> {
    build_derived_view_from_state(
        PipelineState::Flat(BitBuffer::from_bytes(bytes.to_vec())),
        pipeline,
    )
}

pub fn build_derived_view_from_groups(
    groups: &[Vec<u8>],
    pipeline: &FilterPipeline,
) -> Result<DerivedView, String> {
    let state = PipelineState::Grouped(groups.iter().cloned().map(BitBuffer::from_bytes).collect());
    build_derived_view_from_state(state, pipeline)
}

fn build_derived_view_from_state(
    mut state: PipelineState,
    pipeline: &FilterPipeline,
) -> Result<DerivedView, String> {
    for step in &pipeline.steps {
        state = apply_step(state, step)?;
    }

    let groups = match state {
        PipelineState::Flat(buffer) => {
            if buffer.is_empty() {
                Vec::new()
            } else {
                vec![DerivedGroup { data: buffer }]
            }
        }
        PipelineState::Grouped(groups) => groups
            .into_iter()
            .filter(|group| !group.is_empty())
            .map(|data| DerivedGroup { data })
            .collect(),
    };

    Ok(DerivedView::new(groups))
}

fn apply_step(state: PipelineState, step: &FilterStep) -> Result<PipelineState, String> {
    match step {
        FilterStep::ReverseBitsPerByte => Ok(state.map_bytes(u8::reverse_bits)),
        FilterStep::InvertBits => Ok(state.map_bytes(|byte| !byte)),
        FilterStep::XorMask { mask } => Ok(state.map_bytes(|byte| byte ^ mask)),
        FilterStep::Flatten => Ok(PipelineState::Grouped(vec![state.into_flat()])),
        FilterStep::SyncOnPreamble { bits } => {
            let pattern = parse_preamble_bits(bits)?;
            let buffer = state.into_flat();
            let starts = find_group_starts(&buffer, &pattern);

            if starts.is_empty() {
                return Err("Preamble was not found in the current stream.".to_owned());
            }

            let mut groups = Vec::with_capacity(starts.len());
            for (index, start_bit) in starts.iter().copied().enumerate() {
                let end_bit = starts
                    .get(index + 1)
                    .copied()
                    .unwrap_or_else(|| buffer.len_bits());
                let group = buffer.slice_bits(start_bit, end_bit.saturating_sub(start_bit));
                if !group.is_empty() {
                    groups.push(group);
                }
            }

            Ok(PipelineState::Grouped(groups))
        }
        FilterStep::KeepGroupsLongerThanBytes { min_bytes } => match state {
            PipelineState::Flat(_) => Err(
                "Keep-groups filter requires a grouping step earlier in the pipeline.".to_owned(),
            ),
            PipelineState::Grouped(groups) => Ok(PipelineState::Grouped(
                groups
                    .into_iter()
                    .filter(|group| group.len_bits() > min_bytes.saturating_mul(8))
                    .collect(),
            )),
        },
        FilterStep::SelectBitRangeFromGroup {
            start_bit,
            length_bits,
        } => match state {
            PipelineState::Flat(_) => Err(
                "Select-range filter requires a grouping step earlier in the pipeline.".to_owned(),
            ),
            PipelineState::Grouped(groups) => Ok(PipelineState::Grouped(
                groups
                    .into_iter()
                    .map(|group| group.slice_bits(*start_bit, *length_bits))
                    .filter(|group| !group.is_empty())
                    .collect(),
            )),
        },
    }
}

fn parse_preamble_bits(bits: &str) -> Result<Vec<u8>, String> {
    let trimmed = bits.trim();
    if trimmed.is_empty() {
        return Err("Preamble bits cannot be empty.".to_owned());
    }

    if let Some(hex) = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        return parse_hex_preamble_bits(hex);
    }

    trimmed
        .chars()
        .map(|character| match character {
            '0' => Ok(0),
            '1' => Ok(1),
            _ => Err(
                "Preamble bits must contain only 0 or 1, or use a 0x-prefixed hex value."
                    .to_owned(),
            ),
        })
        .collect()
}

fn parse_hex_preamble_bits(hex: &str) -> Result<Vec<u8>, String> {
    if hex.is_empty() {
        return Err("Hex preamble cannot be empty after the 0x prefix.".to_owned());
    }

    let mut bits = Vec::with_capacity(hex.len().saturating_mul(4));
    for character in hex.chars() {
        let nibble = character.to_digit(16).ok_or_else(|| {
            "Hex preamble must contain only hexadecimal digits after the 0x prefix.".to_owned()
        })? as u8;

        for shift in (0..4).rev() {
            bits.push((nibble >> shift) & 1);
        }
    }

    Ok(bits)
}

fn find_group_starts(buffer: &BitBuffer, pattern: &[u8]) -> Vec<usize> {
    if pattern.is_empty() || pattern.len() > buffer.len_bits() {
        return Vec::new();
    }

    let mut starts = Vec::new();
    let mut bit_index = 0usize;
    while bit_index + pattern.len() <= buffer.len_bits() {
        if pattern_matches(buffer, bit_index, pattern) {
            starts.push(bit_index);
            bit_index += pattern.len();
        } else {
            bit_index += 1;
        }
    }

    starts
}

fn pattern_matches(buffer: &BitBuffer, start_bit: usize, pattern: &[u8]) -> bool {
    pattern
        .iter()
        .enumerate()
        .all(|(index, expected)| buffer.bit(start_bit + index) == Some(*expected))
}

fn mask_unused_tail_bits(bytes: &mut [u8], bit_len: usize) {
    let remainder = bit_len % 8;
    if remainder == 0 {
        return;
    }

    if let Some(last_byte) = bytes.last_mut() {
        let mask = u8::MAX << (8 - remainder);
        *last_byte &= mask;
    }
}

#[cfg(test)]
mod tests {
    use super::{DerivedView, FilterPipeline, FilterStep, build_derived_view, parse_preamble_bits};

    fn group_bits(view: &DerivedView) -> Vec<Vec<u8>> {
        view.groups()
            .iter()
            .map(|group| {
                (0..group.len_bits())
                    .map(|bit_index| group.bit(bit_index).unwrap_or(0))
                    .collect()
            })
            .collect()
    }

    #[test]
    fn stacked_group_pipeline_keeps_only_long_enough_groups() {
        let bytes = [0b1010_0011, 0b1010_1111, 0b1010_0001];
        let pipeline = FilterPipeline {
            steps: vec![
                FilterStep::SyncOnPreamble {
                    bits: "1010".to_owned(),
                },
                FilterStep::KeepGroupsLongerThanBytes { min_bytes: 1 },
            ],
        };

        let view = build_derived_view(&bytes, &pipeline).expect("pipeline should succeed");
        assert_eq!(view.group_count(), 0);
    }

    #[test]
    fn stacked_pipeline_reverses_and_selects_bits_per_group() {
        let bytes = [0b1010_0001, 0b1010_1111];
        let pipeline = FilterPipeline {
            steps: vec![
                FilterStep::SyncOnPreamble {
                    bits: "1010".to_owned(),
                },
                FilterStep::ReverseBitsPerByte,
                FilterStep::SelectBitRangeFromGroup {
                    start_bit: 0,
                    length_bits: 4,
                },
            ],
        };

        let view = build_derived_view(&bytes, &pipeline).expect("pipeline should succeed");
        assert_eq!(group_bits(&view), vec![vec![1, 0, 0, 0], vec![1, 1, 1, 1]]);
    }

    #[test]
    fn flatten_concatenates_all_groups_into_one_group() {
        let bytes = [0b1010_0001, 0b1010_1111];
        let pipeline = FilterPipeline {
            steps: vec![
                FilterStep::SyncOnPreamble {
                    bits: "1010".to_owned(),
                },
                FilterStep::Flatten,
            ],
        };

        let view = build_derived_view(&bytes, &pipeline).expect("pipeline should succeed");
        assert_eq!(
            group_bits(&view),
            vec![vec![1, 0, 1, 0, 0, 0, 0, 1, 1, 0, 1, 0, 1, 1, 1, 1]]
        );
    }

    #[test]
    fn flatten_keeps_flat_input_as_single_group() {
        let bytes = [0b1111_0000];
        let pipeline = FilterPipeline {
            steps: vec![FilterStep::Flatten],
        };

        let view = build_derived_view(&bytes, &pipeline).expect("pipeline should succeed");
        assert_eq!(group_bits(&view), vec![vec![1, 1, 1, 1, 0, 0, 0, 0]]);
    }

    #[test]
    fn grouped_input_preserves_packet_boundaries_without_filters() {
        let groups = vec![vec![0xAA], vec![0x55, 0xF0]];

        let view = super::build_derived_view_from_groups(&groups, &Default::default())
            .expect("grouped source should succeed");

        assert_eq!(
            group_bits(&view),
            vec![
                vec![1, 0, 1, 0, 1, 0, 1, 0],
                vec![0, 1, 0, 1, 0, 1, 0, 1, 1, 1, 1, 1, 0, 0, 0, 0],
            ]
        );
    }

    #[test]
    fn group_only_filters_require_grouping_first() {
        let bytes = [0b1111_0000];
        let pipeline = FilterPipeline {
            steps: vec![FilterStep::SelectBitRangeFromGroup {
                start_bit: 0,
                length_bits: 4,
            }],
        };

        let error = build_derived_view(&bytes, &pipeline).expect_err("pipeline should fail");
        assert!(error.contains("requires a grouping step"));
    }

    #[test]
    fn parse_preamble_bits_accepts_hex_input() {
        let bits = parse_preamble_bits("0xA5").expect("hex preamble should parse");

        assert_eq!(bits, vec![1, 0, 1, 0, 0, 1, 0, 1]);
    }

    #[test]
    fn sync_on_preamble_accepts_hex_input() {
        let bytes = [0b1010_0001, 0b1010_1111];
        let pipeline = FilterPipeline {
            steps: vec![FilterStep::SyncOnPreamble {
                bits: "0xA".to_owned(),
            }],
        };

        let view = build_derived_view(&bytes, &pipeline).expect("pipeline should succeed");

        assert_eq!(
            group_bits(&view),
            vec![vec![1, 0, 1, 0, 0, 0, 0, 1], vec![1, 0, 1, 0, 1, 1, 1, 1],]
        );
    }
}
