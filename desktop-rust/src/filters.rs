#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FilterPipeline {
    pub steps: Vec<FilterStep>,
}

impl FilterPipeline {
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum L2Protocol {
    #[default]
    Ethernet,
    PppAsync,
    PppHdlcLike,
    Hdlc,
    Sdlc,
    CiscoHdlc,
}

impl L2Protocol {
    pub const ALL: [Self; 6] = [
        Self::Ethernet,
        Self::PppAsync,
        Self::PppHdlcLike,
        Self::Hdlc,
        Self::Sdlc,
        Self::CiscoHdlc,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Ethernet => "Ethernet",
            Self::PppAsync => "PPP (async)",
            Self::PppHdlcLike => "PPP (HDLC-like)",
            Self::Hdlc => "HDLC",
            Self::Sdlc => "SDLC",
            Self::CiscoHdlc => "Cisco HDLC",
        }
    }

    fn no_packets_error(self) -> String {
        format!(
            "No {} packets were found in the current stream.",
            self.label()
        )
    }

    pub fn cycle(self, delta: isize) -> Self {
        let protocols = Self::ALL;
        let current_index = protocols
            .iter()
            .position(|candidate| *candidate == self)
            .unwrap_or(0);
        let len = protocols.len() as isize;
        let next_index = (current_index as isize + delta).rem_euclid(len) as usize;
        protocols[next_index]
    }

    fn requires_byte_alignment(self) -> bool {
        matches!(self, Self::Ethernet | Self::PppAsync)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FilterStep {
    SyncOnPreamble {
        bits: String,
    },
    Split {
        group_size_bits: usize,
    },
    Chop {
        bits: usize,
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
    ExtractL2Packets {
        protocol: L2Protocol,
    },
}

impl FilterStep {
    pub fn label(&self) -> &'static str {
        match self {
            Self::SyncOnPreamble { .. } => "Sync on preamble",
            Self::Split { .. } => "Split",
            Self::Chop { .. } => "Chop",
            Self::ReverseBitsPerByte => "Reverse bits in each byte",
            Self::InvertBits => "Invert bits",
            Self::XorMask { .. } => "XOR byte mask",
            Self::Flatten => "Flatten groups",
            Self::KeepGroupsLongerThanBytes { .. } => "Keep groups longer than bytes",
            Self::SelectBitRangeFromGroup { .. } => "Select bit range from group",
            Self::ExtractL2Packets { .. } => "Extract L2 packets",
        }
    }

    pub fn help_text(&self) -> &'static str {
        match self {
            Self::SyncOnPreamble { .. } => {
                "Split the current bitstream into groups whenever the preamble pattern is found."
            }
            Self::Split { .. } => {
                "Flatten the current view, then cut it into fixed-size groups and keep any partial tail group."
            }
            Self::Chop { .. } => {
                "Remove a fixed number of bits from the start of the file, or from the start of each group once groups exist."
            }
            Self::ReverseBitsPerByte => {
                "Flip the bit order inside every byte without changing the byte positions."
            }
            Self::InvertBits => "Change every 0 bit to 1 and every 1 bit to 0.",
            Self::XorMask { .. } => {
                "XOR every byte with a mask to toggle selected bit positions consistently across the view."
            }
            Self::Flatten => {
                "Concatenate all current groups into one continuous group while preserving the visible bit order."
            }
            Self::KeepGroupsLongerThanBytes { .. } => {
                "Drop any group whose length is not greater than the configured byte threshold."
            }
            Self::SelectBitRangeFromGroup { .. } => {
                "Keep only a fixed bit range from each group and discard the rest."
            }
            Self::ExtractL2Packets { .. } => {
                "Split the current stream into packet groups using Ethernet preambles, PPP byte-stuffed flags, or HDLC-family bit-stuffed flags."
            }
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

    fn from_bits(bits: impl IntoIterator<Item = u8>) -> Self {
        let mut buffer = Self::default();
        for bit in bits {
            buffer.push_bit(bit);
        }
        buffer
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

    fn is_byte_aligned(&self) -> bool {
        self.bit_len % 8 == 0
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
        FilterStep::Split { group_size_bits } => {
            if *group_size_bits == 0 {
                return Err("Split filter requires a group size greater than zero bits.".to_owned());
            }

            let buffer = state.into_flat();
            let mut groups = Vec::with_capacity(buffer.len_bits().div_ceil(*group_size_bits));
            let mut start_bit = 0usize;
            while start_bit < buffer.len_bits() {
                let length_bits = (*group_size_bits).min(buffer.len_bits() - start_bit);
                let group = buffer.slice_bits(start_bit, length_bits);
                if !group.is_empty() {
                    groups.push(group);
                }
                start_bit += *group_size_bits;
            }

            Ok(PipelineState::Grouped(groups))
        }
        FilterStep::Chop { bits } => match state {
            PipelineState::Flat(buffer) => Ok(PipelineState::Flat(
                buffer.slice_bits(*bits, buffer.len_bits().saturating_sub(*bits)),
            )),
            PipelineState::Grouped(groups) => Ok(PipelineState::Grouped(
                groups
                    .into_iter()
                    .map(|group| group.slice_bits(*bits, group.len_bits().saturating_sub(*bits)))
                    .filter(|group| !group.is_empty())
                    .collect(),
            )),
        },
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
        FilterStep::ExtractL2Packets { protocol } => extract_l2_packets(state, *protocol),
    }
}

fn extract_l2_packets(state: PipelineState, protocol: L2Protocol) -> Result<PipelineState, String> {
    let groups = match state {
        PipelineState::Flat(buffer) => vec![buffer],
        PipelineState::Grouped(groups) => groups,
    };

    let mut extracted = Vec::new();
    for group in groups {
        if group.is_empty() {
            continue;
        }

        if protocol.requires_byte_alignment() && !group.is_byte_aligned() {
            return Err(format!(
                "{} extraction requires byte-aligned input. Current group length is {} bits.",
                protocol.label(),
                group.len_bits()
            ));
        }

        extracted.extend(extract_packets_from_group(&group, protocol)?);
    }

    if extracted.is_empty() {
        return Err(protocol.no_packets_error());
    }

    Ok(PipelineState::Grouped(extracted))
}

fn extract_packets_from_group(
    group: &BitBuffer,
    protocol: L2Protocol,
) -> Result<Vec<BitBuffer>, String> {
    match protocol {
        L2Protocol::Ethernet => Ok(extract_ethernet_packets(group)),
        L2Protocol::PppAsync => Ok(extract_ppp_async_packets(group)),
        L2Protocol::PppHdlcLike | L2Protocol::Hdlc | L2Protocol::Sdlc | L2Protocol::CiscoHdlc => {
            Ok(extract_hdlc_like_packets(group))
        }
    }
}

const ETHERNET_PREAMBLE_BYTES: [u8; 8] = [0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0xD5];
const ETHERNET_MIN_FRAME_BYTES: usize = 64;
const ETHERNET_MAX_FRAME_BYTES: usize = 1_522;
const ETHERNET_VLAN_ETHERTYPES: [u16; 3] = [0x8100, 0x88A8, 0x9100];
const PPP_FLAG_BYTE: u8 = 0x7E;
const PPP_ESCAPE_BYTE: u8 = 0x7D;
const HDLC_FLAG_BITS: [u8; 8] = [0, 1, 1, 1, 1, 1, 1, 0];
const CRC32_REVERSED_POLYNOMIAL: u32 = 0xEDB8_8320;
const CRC16_CCITT_REVERSED_POLYNOMIAL: u16 = 0x8408;

fn extract_ethernet_packets(group: &BitBuffer) -> Vec<BitBuffer> {
    let bytes = group.bytes();
    let starts = find_byte_pattern(bytes, &ETHERNET_PREAMBLE_BYTES);
    let mut packets = Vec::new();
    let mut accepted_until = 0usize;

    for start in starts.iter().copied() {
        if start < accepted_until {
            continue;
        }

        let frame_start = start + ETHERNET_PREAMBLE_BYTES.len();
        if frame_start >= bytes.len() {
            continue;
        }

        let mut candidate_lengths = Vec::new();

        if let Some(explicit_length) = ethernet_frame_length_from_header(bytes, frame_start)
            .filter(|frame_len| frame_start.saturating_add(*frame_len) <= bytes.len())
        {
            push_unique(&mut candidate_lengths, explicit_length);
        }

        for next_start in starts
            .iter()
            .copied()
            .filter(|next_start| *next_start > start)
        {
            let frame_len = next_start.saturating_sub(frame_start);
            if !(ETHERNET_MIN_FRAME_BYTES..=ETHERNET_MAX_FRAME_BYTES).contains(&frame_len) {
                continue;
            }
            push_unique(&mut candidate_lengths, frame_len);
        }

        let remaining = bytes.len().saturating_sub(frame_start);
        if (ETHERNET_MIN_FRAME_BYTES..=ETHERNET_MAX_FRAME_BYTES).contains(&remaining) {
            push_unique(&mut candidate_lengths, remaining);
        }

        for frame_len in candidate_lengths {
            let frame_end = frame_start + frame_len;
            let frame = &bytes[frame_start..frame_end];
            if is_good_ethernet_frame(frame) {
                packets.push(BitBuffer::from_bytes(frame.to_vec()));
                accepted_until = frame_end;
                break;
            }
        }
    }

    packets
}

fn ethernet_frame_length_from_header(bytes: &[u8], frame_start: usize) -> Option<usize> {
    let mut header_len = 14usize;
    let mut field_offset = frame_start + 12;

    loop {
        let value = read_u16_be(bytes, field_offset)?;
        if ETHERNET_VLAN_ETHERTYPES.contains(&value) {
            header_len += 4;
            field_offset += 4;
            continue;
        }

        if value <= 1_500 {
            return Some((header_len + value as usize + 4).max(ETHERNET_MIN_FRAME_BYTES));
        }

        return None;
    }
}

fn extract_ppp_async_packets(group: &BitBuffer) -> Vec<BitBuffer> {
    let bytes = group.bytes();
    let mut packets = Vec::new();
    let mut current_start = None;

    for (index, byte) in bytes.iter().copied().enumerate() {
        if byte != PPP_FLAG_BYTE {
            continue;
        }

        if let Some(start) = current_start
            && start < index
            && let Some(packet) = decode_ppp_async_packet(&bytes[start..index])
        {
            packets.push(BitBuffer::from_bytes(packet));
        }

        current_start = Some(index + 1);
    }

    packets
}

fn decode_ppp_async_packet(bytes: &[u8]) -> Option<Vec<u8>> {
    if bytes.is_empty() {
        return None;
    }

    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0usize;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte == PPP_ESCAPE_BYTE {
            index += 1;
            let escaped = *bytes.get(index)?;
            decoded.push(escaped ^ 0x20);
        } else {
            decoded.push(byte);
        }
        index += 1;
    }

    (!decoded.is_empty()).then_some(decoded)
}

fn extract_hdlc_like_packets(group: &BitBuffer) -> Vec<BitBuffer> {
    let starts = find_group_starts(group, &HDLC_FLAG_BITS);
    let mut packets = Vec::new();

    for window in starts.windows(2) {
        let start_bit = window[0] + HDLC_FLAG_BITS.len();
        let end_bit = window[1];
        if end_bit <= start_bit {
            continue;
        }

        let stuffed = group.slice_bits(start_bit, end_bit - start_bit);
        let Some(packet) = destuff_hdlc_bits(&stuffed) else {
            continue;
        };
        if is_good_hdlc_frame(&packet) {
            packets.push(packet);
        }
    }

    packets
}

fn destuff_hdlc_bits(stuffed: &BitBuffer) -> Option<BitBuffer> {
    let mut bits = Vec::with_capacity(stuffed.len_bits());
    let mut consecutive_ones = 0usize;

    for bit_index in 0..stuffed.len_bits() {
        let bit = stuffed.bit(bit_index)?;
        match bit {
            1 => {
                consecutive_ones += 1;
                if consecutive_ones > 5 {
                    return None;
                }
                bits.push(1);
            }
            0 => {
                if consecutive_ones == 5 {
                    consecutive_ones = 0;
                    continue;
                }
                consecutive_ones = 0;
                bits.push(0);
            }
            _ => return None,
        }
    }

    Some(BitBuffer::from_bits(bits))
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

fn find_byte_pattern(bytes: &[u8], pattern: &[u8]) -> Vec<usize> {
    if pattern.is_empty() || pattern.len() > bytes.len() {
        return Vec::new();
    }

    let mut starts = Vec::new();
    let mut index = 0usize;
    while index + pattern.len() <= bytes.len() {
        if &bytes[index..index + pattern.len()] == pattern {
            starts.push(index);
            index += pattern.len();
        } else {
            index += 1;
        }
    }

    starts
}

fn read_u16_be(bytes: &[u8], start: usize) -> Option<u16> {
    let high = *bytes.get(start)? as u16;
    let low = *bytes.get(start + 1)? as u16;
    Some((high << 8) | low)
}

fn read_u16_le(bytes: &[u8], start: usize) -> Option<u16> {
    let low = *bytes.get(start)? as u16;
    let high = *bytes.get(start + 1)? as u16;
    Some(low | (high << 8))
}

fn read_u32_le(bytes: &[u8], start: usize) -> Option<u32> {
    let b0 = *bytes.get(start)? as u32;
    let b1 = *bytes.get(start + 1)? as u32;
    let b2 = *bytes.get(start + 2)? as u32;
    let b3 = *bytes.get(start + 3)? as u32;
    Some(b0 | (b1 << 8) | (b2 << 16) | (b3 << 24))
}

fn push_unique(values: &mut Vec<usize>, value: usize) {
    if !values.contains(&value) {
        values.push(value);
    }
}

fn is_good_ethernet_frame(frame: &[u8]) -> bool {
    if !(ETHERNET_MIN_FRAME_BYTES..=ETHERNET_MAX_FRAME_BYTES).contains(&frame.len()) {
        return false;
    }

    let frame_without_fcs_len = frame.len().saturating_sub(4);
    if frame_without_fcs_len < 14 {
        return false;
    }

    let Some(actual_fcs) = read_u32_le(frame, frame_without_fcs_len) else {
        return false;
    };
    let expected_fcs = crc32_fcs(&frame[..frame_without_fcs_len]);

    actual_fcs == expected_fcs
}

fn is_good_hdlc_frame(frame: &BitBuffer) -> bool {
    if frame.is_empty() || !frame.is_byte_aligned() {
        return false;
    }

    let bytes = frame.bytes();
    has_valid_hdlc_fcs16(bytes) || has_valid_hdlc_fcs32(bytes)
}

fn has_valid_hdlc_fcs16(frame: &[u8]) -> bool {
    if frame.len() < 3 {
        return false;
    }

    let payload_len = frame.len() - 2;
    let Some(actual_fcs) = read_u16_le(frame, payload_len) else {
        return false;
    };

    actual_fcs == crc16_ccitt_fcs(&frame[..payload_len])
}

fn has_valid_hdlc_fcs32(frame: &[u8]) -> bool {
    if frame.len() < 5 {
        return false;
    }

    let payload_len = frame.len() - 4;
    let Some(actual_fcs) = read_u32_le(frame, payload_len) else {
        return false;
    };

    actual_fcs == crc32_fcs(&frame[..payload_len])
}

fn crc16_ccitt_fcs(bytes: &[u8]) -> u16 {
    let mut crc = 0xFFFFu16;

    for &byte in bytes {
        let mut current = byte as u16;
        for _ in 0..8 {
            let mix = (crc ^ current) & 1;
            crc >>= 1;
            if mix != 0 {
                crc ^= CRC16_CCITT_REVERSED_POLYNOMIAL;
            }
            current >>= 1;
        }
    }

    !crc
}

fn crc32_fcs(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;

    for &byte in bytes {
        let mut current = byte as u32;
        for _ in 0..8 {
            let mix = (crc ^ current) & 1;
            crc >>= 1;
            if mix != 0 {
                crc ^= CRC32_REVERSED_POLYNOMIAL;
            }
            current >>= 1;
        }
    }

    !crc
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
    use super::{
        DerivedView, FilterPipeline, FilterStep, L2Protocol, build_derived_view,
        parse_preamble_bits,
    };

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

    fn group_bytes(view: &DerivedView) -> Vec<Vec<u8>> {
        view.groups()
            .iter()
            .map(|group| group.packed_bytes().to_vec())
            .collect()
    }

    fn ethernet_length_frame(payload: &[u8], filler: u8) -> Vec<u8> {
        let mut frame_without_fcs = vec![
            0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25,
        ];
        frame_without_fcs.extend_from_slice(&(payload.len() as u16).to_be_bytes());
        frame_without_fcs.extend_from_slice(payload);

        let min_without_fcs = super::ETHERNET_MIN_FRAME_BYTES - 4;
        if frame_without_fcs.len() < min_without_fcs {
            frame_without_fcs.resize(min_without_fcs, filler);
        }

        let mut frame = frame_without_fcs.clone();
        frame.extend_from_slice(&super::crc32_fcs(&frame_without_fcs).to_le_bytes());
        frame
    }

    fn bits_to_bytes(bits: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(bits.len().div_ceil(8));
        for (index, bit) in bits.iter().copied().enumerate() {
            if index % 8 == 0 {
                bytes.push(0);
            }
            if bit != 0 {
                let shift = 7 - (index % 8);
                let last_index = bytes.len() - 1;
                bytes[last_index] |= 1 << shift;
            }
        }
        bytes
    }

    fn bytes_to_bits(bytes: &[u8]) -> Vec<u8> {
        let mut bits = Vec::with_capacity(bytes.len().saturating_mul(8));
        for &byte in bytes {
            for shift in (0..8).rev() {
                bits.push((byte >> shift) & 1);
            }
        }
        bits
    }

    fn hdlc_frame_bytes(payload: &[u8]) -> Vec<u8> {
        let mut frame = payload.to_vec();
        frame.extend_from_slice(&super::crc16_ccitt_fcs(payload).to_le_bytes());
        frame
    }

    fn hdlc_wrap_bytes(frame: &[u8]) -> Vec<u8> {
        let payload_bits = bytes_to_bits(frame);
        let mut stuffed = Vec::with_capacity(payload_bits.len() + 16);
        stuffed.extend_from_slice(&super::HDLC_FLAG_BITS);

        let mut consecutive_ones = 0usize;
        for bit in payload_bits {
            stuffed.push(bit);
            if bit == 1 {
                consecutive_ones += 1;
                if consecutive_ones == 5 {
                    stuffed.push(0);
                    consecutive_ones = 0;
                }
            } else {
                consecutive_ones = 0;
            }
        }

        stuffed.extend_from_slice(&super::HDLC_FLAG_BITS);
        bits_to_bytes(&stuffed)
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
    fn split_regroups_flattened_stream_at_fixed_bit_width() {
        let groups = vec![vec![0xAA], vec![0x55, 0xF0]];
        let pipeline = FilterPipeline {
            steps: vec![FilterStep::Split { group_size_bits: 6 }],
        };

        let view = super::build_derived_view_from_groups(&groups, &pipeline)
            .expect("split should flatten and regroup");

        assert_eq!(
            group_bits(&view),
            vec![
                vec![1, 0, 1, 0, 1, 0],
                vec![1, 0, 0, 1, 0, 1],
                vec![0, 1, 0, 1, 1, 1],
                vec![1, 1, 0, 0, 0, 0],
            ]
        );
    }

    #[test]
    fn split_preserves_partial_tail_group() {
        let bytes = [0b1101_0011];
        let pipeline = FilterPipeline {
            steps: vec![FilterStep::Split { group_size_bits: 3 }],
        };

        let view = build_derived_view(&bytes, &pipeline).expect("split should succeed");

        assert_eq!(
            group_bits(&view),
            vec![vec![1, 1, 0], vec![1, 0, 0], vec![1, 1]]
        );
    }

    #[test]
    fn split_rejects_zero_sized_groups() {
        let bytes = [0b1111_0000];
        let pipeline = FilterPipeline {
            steps: vec![FilterStep::Split { group_size_bits: 0 }],
        };

        let error = build_derived_view(&bytes, &pipeline).expect_err("split should fail");
        assert!(error.contains("greater than zero bits"));
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
    fn chop_removes_prefix_once_when_input_is_still_flat() {
        let bytes = [0b1101_0011];
        let pipeline = FilterPipeline {
            steps: vec![FilterStep::Chop { bits: 3 }],
        };

        let view = build_derived_view(&bytes, &pipeline).expect("pipeline should succeed");

        assert_eq!(group_bits(&view), vec![vec![1, 0, 0, 1, 1]]);
    }

    #[test]
    fn chop_removes_prefix_from_each_existing_group() {
        let groups = vec![vec![0xAA], vec![0x55, 0xF0]];
        let pipeline = FilterPipeline {
            steps: vec![FilterStep::Chop { bits: 4 }],
        };

        let view = super::build_derived_view_from_groups(&groups, &pipeline)
            .expect("pipeline should succeed");

        assert_eq!(
            group_bits(&view),
            vec![vec![1, 0, 1, 0], vec![0, 1, 0, 1, 1, 1, 1, 1, 0, 0, 0, 0],]
        );
    }

    #[test]
    fn chop_removes_prefix_from_groups_created_earlier_in_pipeline() {
        let bytes = [0b1010_0001, 0b1010_1111];
        let pipeline = FilterPipeline {
            steps: vec![
                FilterStep::SyncOnPreamble {
                    bits: "1010".to_owned(),
                },
                FilterStep::Chop { bits: 4 },
            ],
        };

        let view = build_derived_view(&bytes, &pipeline).expect("pipeline should succeed");

        assert_eq!(group_bits(&view), vec![vec![0, 0, 0, 1], vec![1, 1, 1, 1],]);
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

    #[test]
    fn extract_l2_packets_splits_ethernet_frames_on_preamble() {
        let frame_one = ethernet_length_frame(&[0xDE, 0xAD, 0xBE, 0xEF], 0xA1);
        let frame_two = ethernet_length_frame(&[0x01, 0x02, 0x03, 0x04, 0x05], 0xB2);

        let mut bytes = Vec::new();
        bytes.extend_from_slice(&super::ETHERNET_PREAMBLE_BYTES);
        bytes.extend_from_slice(&frame_one);
        bytes.extend_from_slice(&super::ETHERNET_PREAMBLE_BYTES);
        bytes.extend_from_slice(&frame_two);

        let pipeline = FilterPipeline {
            steps: vec![FilterStep::ExtractL2Packets {
                protocol: L2Protocol::Ethernet,
            }],
        };

        let view = build_derived_view(&bytes, &pipeline).expect("ethernet extraction should work");

        assert_eq!(group_bytes(&view), vec![frame_one, frame_two]);
    }

    #[test]
    fn extract_l2_packets_decodes_async_ppp_escapes() {
        let bytes = [
            0x7E, 0xFF, 0x03, 0x00, 0x21, 0x7D, 0x5E, 0x7D, 0x5D, 0x45, 0x12, 0x34, 0x7E,
        ];
        let pipeline = FilterPipeline {
            steps: vec![FilterStep::ExtractL2Packets {
                protocol: L2Protocol::PppAsync,
            }],
        };

        let view = build_derived_view(&bytes, &pipeline).expect("ppp extraction should work");

        assert_eq!(
            group_bytes(&view),
            vec![vec![0xFF, 0x03, 0x00, 0x21, 0x7E, 0x7D, 0x45, 0x12, 0x34]]
        );
    }

    #[test]
    fn extract_l2_packets_destuffs_hdlc_payload_bits() {
        let frame = hdlc_frame_bytes(&[0xF8, 0xA0]);
        let bytes = hdlc_wrap_bytes(&frame);
        let pipeline = FilterPipeline {
            steps: vec![FilterStep::ExtractL2Packets {
                protocol: L2Protocol::Hdlc,
            }],
        };

        let view = build_derived_view(&bytes, &pipeline).expect("hdlc extraction should work");

        assert_eq!(group_bytes(&view), vec![frame]);
    }

    #[test]
    fn extract_l2_packets_requires_byte_alignment_for_ethernet() {
        let groups = vec![vec![0x55, 0x55]];
        let pipeline = FilterPipeline {
            steps: vec![
                FilterStep::SyncOnPreamble {
                    bits: "101".to_owned(),
                },
                FilterStep::ExtractL2Packets {
                    protocol: L2Protocol::Ethernet,
                },
            ],
        };

        let error = super::build_derived_view_from_groups(&groups, &pipeline)
            .expect_err("misaligned ethernet extraction should fail");

        assert!(error.contains("byte-aligned"));
    }

    #[test]
    fn extract_l2_packets_drops_ethernet_frames_with_bad_fcs() {
        let mut bad_frame = ethernet_length_frame(&[0xAA, 0xBB, 0xCC], 0x11);
        let last_index = bad_frame.len() - 1;
        bad_frame[last_index] ^= 0xFF;

        let good_frame = ethernet_length_frame(&[0xDE, 0xAD, 0xBE, 0xEF], 0x22);

        let mut bytes = Vec::new();
        bytes.extend_from_slice(&super::ETHERNET_PREAMBLE_BYTES);
        bytes.extend_from_slice(&bad_frame);
        bytes.extend_from_slice(&super::ETHERNET_PREAMBLE_BYTES);
        bytes.extend_from_slice(&good_frame);

        let pipeline = FilterPipeline {
            steps: vec![FilterStep::ExtractL2Packets {
                protocol: L2Protocol::Ethernet,
            }],
        };

        let view =
            build_derived_view(&bytes, &pipeline).expect("good ethernet frame should remain");

        assert_eq!(group_bytes(&view), vec![good_frame]);
    }

    #[test]
    fn extract_l2_packets_drops_hdlc_frames_with_bad_fcs() {
        let mut bad_frame = hdlc_frame_bytes(&[0xA5, 0x5A]);
        let last_index = bad_frame.len() - 1;
        bad_frame[last_index] ^= 0xFF;

        let good_frame = hdlc_frame_bytes(&[0xF8, 0xA0]);

        let mut bytes = Vec::new();
        bytes.extend_from_slice(&hdlc_wrap_bytes(&bad_frame));
        bytes.extend_from_slice(&hdlc_wrap_bytes(&good_frame));

        let pipeline = FilterPipeline {
            steps: vec![FilterStep::ExtractL2Packets {
                protocol: L2Protocol::Hdlc,
            }],
        };

        let view = build_derived_view(&bytes, &pipeline).expect("good hdlc frame should remain");

        assert_eq!(group_bytes(&view), vec![good_frame]);
    }
}
