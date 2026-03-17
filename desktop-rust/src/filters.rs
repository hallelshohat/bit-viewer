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
        bits: i64,
    },
    ReverseBitsPerByte,
    InvertBits,
    XorMask {
        mask: u8,
    },
    LfsrScramble {
        seed: String,
        polynomial: String,
    },
    LfsrDescramble {
        seed: String,
        polynomial: String,
    },
    Flatten,
    KeepGroupsLongerThanBytes {
        min_bytes: usize,
    },
    SelectBitRangeFromGroup {
        start_bit: usize,
        length_bits: usize,
    },
    SelectSubgroupRangesFromGroup {
        chunk_count: usize,
        subgroup_size_bits: usize,
        subgroup_ranges: Vec<GroupChunkRange>,
    },
    ExtractL2Packets {
        protocol: L2Protocol,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GroupChunkRange {
    pub start_chunk: usize,
    pub end_chunk: usize,
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
            Self::LfsrScramble { .. } => "LFSR scramble",
            Self::LfsrDescramble { .. } => "LFSR descramble",
            Self::Flatten => "Flatten groups",
            Self::KeepGroupsLongerThanBytes { .. } => "Keep groups longer than bytes",
            Self::SelectBitRangeFromGroup { .. } => "Select bit range from group",
            Self::SelectSubgroupRangesFromGroup { .. } => "Select subgroup ranges from group",
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
                "Remove bits from the start when the value is positive, or from the end when the value is negative. Once groups exist, apply the trim to each group."
            }
            Self::ReverseBitsPerByte => {
                "Flip the bit order inside every byte without changing the byte positions."
            }
            Self::InvertBits => "Change every 0 bit to 1 and every 1 bit to 0.",
            Self::XorMask { .. } => {
                "XOR every byte with a mask to toggle selected bit positions consistently across the view."
            }
            Self::LfsrScramble { .. } => {
                "Scramble each bit with a self-synchronizing LFSR using a polynomial such as x^7+x^3+1."
            }
            Self::LfsrDescramble { .. } => {
                "Descramble a self-synchronizing LFSR stream using the same seed and polynomial, for example x^7+x^3+1."
            }
            Self::Flatten => {
                "Concatenate all current groups into one continuous group while preserving the visible bit order."
            }
            Self::KeepGroupsLongerThanBytes { .. } => {
                "Drop any group whose length is not greater than the configured byte threshold."
            }
            Self::SelectBitRangeFromGroup { .. } => {
                "Keep only a fixed bit range from each group and discard the rest. The `select` command can also keep ranges of fixed-size virtual subgroups."
            }
            Self::SelectSubgroupRangesFromGroup { .. } => {
                "Split each group into fixed-size virtual subgroups, keep the requested subgroup ranges, and concatenate them back into one group."
            }
            Self::ExtractL2Packets { .. } => {
                "Split the current stream into packet groups using Ethernet preambles, PPP byte-stuffed flags, or HDLC-family bit-stuffed flags."
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FilterCommandSpec {
    pub name: &'static str,
    pub usage: &'static str,
    pub summary: &'static str,
    pub example: &'static str,
}

const FILTER_COMMAND_SPECS: [FilterCommandSpec; 12] = [
    FilterCommandSpec {
        name: "sync",
        usage: "sync <bits>",
        summary: "Split the stream into groups on a preamble bit pattern.",
        example: "sync 1010",
    },
    FilterCommandSpec {
        name: "split",
        usage: "split <group_size_bits>",
        summary: "Cut the flattened stream into fixed-size groups.",
        example: "split 256",
    },
    FilterCommandSpec {
        name: "chop",
        usage: "chop <bits>",
        summary: "Remove bits from the start with positive values or from the end with negative values.",
        example: "chop -8",
    },
    FilterCommandSpec {
        name: "reverse",
        usage: "reverse",
        summary: "Reverse bit order inside each byte.",
        example: "reverse",
    },
    FilterCommandSpec {
        name: "invert",
        usage: "invert",
        summary: "Flip every bit in the current view.",
        example: "invert",
    },
    FilterCommandSpec {
        name: "xor",
        usage: "xor <mask>",
        summary: "XOR every byte with a mask.",
        example: "xor 0xff",
    },
    FilterCommandSpec {
        name: "scramble",
        usage: "scramble <seed> <polynomial>",
        summary: "Scramble bits with a self-synchronizing LFSR.",
        example: "scramble 0x7f x^7+x^3+1",
    },
    FilterCommandSpec {
        name: "descramble",
        usage: "descramble <seed> <polynomial>",
        summary: "Descramble bits with a self-synchronizing LFSR.",
        example: "descramble 0x7f x^7+x^3+1",
    },
    FilterCommandSpec {
        name: "flatten",
        usage: "flatten",
        summary: "Merge all groups back into one continuous stream.",
        example: "flatten",
    },
    FilterCommandSpec {
        name: "keep",
        usage: "keep <min_bytes>",
        summary: "Keep only groups longer than the given byte threshold.",
        example: "keep 6",
    },
    FilterCommandSpec {
        name: "select",
        usage: "select <start_bit> <length_bits> | <chunks>*<bits_per_chunk> <ranges>",
        summary: "Keep a fixed bit range from each group, or keep and concatenate ranges of virtual subgroups.",
        example: "select 32*8 1-16,17-31",
    },
    FilterCommandSpec {
        name: "extract",
        usage: "extract <ethernet|ppp|ppp-hdlc|hdlc|sdlc|cisco-hdlc>",
        summary: "Split the stream into L2 packets for a protocol.",
        example: "extract ethernet",
    },
];

pub fn filter_command_specs() -> &'static [FilterCommandSpec] {
    &FILTER_COMMAND_SPECS
}

pub fn filter_command_suggestions(input: &str) -> Vec<&'static FilterCommandSpec> {
    let query = input.trim().to_ascii_lowercase();
    filter_command_specs()
        .iter()
        .enumerate()
        .filter(|(index, spec)| {
            query.is_empty()
                || spec.name.contains(&query)
                || spec.usage.contains(&query)
                || spec.example.contains(&query)
                || filter_command_aliases(*index)
                    .iter()
                    .any(|alias| alias.contains(&query))
        })
        .map(|(_, spec)| spec)
        .collect()
}

pub fn complete_filter_command(input: &str) -> Option<String> {
    let trimmed_start = input.trim_start();
    if trimmed_start.is_empty() {
        return None;
    }

    let leading_whitespace = &input[..input.len().saturating_sub(trimmed_start.len())];
    let (command_fragment, remainder) = trimmed_start
        .split_once(char::is_whitespace)
        .map_or((trimmed_start, ""), |(command, tail)| (command, tail));

    if !remainder.trim().is_empty() {
        return None;
    }

    let normalized_fragment = command_fragment.to_ascii_lowercase();
    let mut matches = filter_command_specs()
        .iter()
        .enumerate()
        .filter(|(index, spec)| {
            spec.name.starts_with(&normalized_fragment)
                || filter_command_aliases(*index)
                    .iter()
                    .any(|alias| alias.starts_with(&normalized_fragment))
        })
        .map(|(_, spec)| spec.name)
        .collect::<Vec<_>>();

    matches.sort_unstable();
    matches.dedup();

    if matches.len() != 1 {
        return None;
    }

    Some(format!("{leading_whitespace}{} ", matches[0]))
}

pub fn parse_filter_command(input: &str) -> Result<FilterStep, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("Type a filter command such as `split 256` or `chop 8`.".to_owned());
    }

    for (index, _) in FILTER_COMMAND_SPECS.iter().enumerate() {
        if let Some(arguments) = split_command_prefix(trimmed, filter_command_aliases(index)) {
            return parse_filter_command_with_index(index, arguments);
        }
    }

    let known = filter_command_specs()
        .iter()
        .map(|spec| spec.name)
        .collect::<Vec<_>>()
        .join(", ");
    Err(format!("Unknown filter command. Try one of: {known}."))
}

fn filter_command_aliases(index: usize) -> &'static [&'static str] {
    match index {
        0 => &["sync-on-preamble", "sync-preamble", "preamble", "sync"],
        1 => &["split"],
        2 => &["chop"],
        3 => &[
            "reverse-bits-per-byte",
            "reverse-bits",
            "reverse-bytes",
            "reverse",
        ],
        4 => &["invert-bits", "invert"],
        5 => &["xor-mask", "mask", "xor"],
        6 => &["lfsr-scramble", "scrambler", "scramble"],
        7 => &["lfsr-descramble", "descrambler", "descramble"],
        8 => &["flatten-groups", "flatten"],
        9 => &[
            "keep-groups-longer-than-bytes",
            "keep-groups",
            "keep-groups-bytes",
            "keep",
        ],
        10 => &["select-range", "select-bit-range", "range", "select"],
        11 => &["extract-l2", "extract-packets", "packets", "l2", "extract"],
        _ => &[],
    }
}

fn split_command_prefix<'a>(input: &'a str, aliases: &[&str]) -> Option<&'a str> {
    let trimmed = input.trim_start();
    for alias in aliases {
        let Some((candidate, tail)) = trimmed.split_at_checked(alias.len()) else {
            continue;
        };
        if !candidate.eq_ignore_ascii_case(alias) {
            continue;
        }
        if tail.is_empty() || tail.starts_with(char::is_whitespace) {
            return Some(tail.trim_start());
        }
    }
    None
}

fn parse_filter_command_with_index(index: usize, arguments: &str) -> Result<FilterStep, String> {
    match index {
        0 => {
            let bits = if arguments.is_empty() {
                "1010".to_owned()
            } else {
                arguments.to_owned()
            };
            parse_preamble_bits(&bits)?;
            Ok(FilterStep::SyncOnPreamble { bits })
        }
        1 => Ok(FilterStep::Split {
            group_size_bits: parse_required_positive_usize(arguments, "split", 8)?,
        }),
        2 => Ok(FilterStep::Chop {
            bits: parse_optional_i64(arguments, "chop", 8)?,
        }),
        3 => {
            reject_extra_arguments(arguments, "reverse")?;
            Ok(FilterStep::ReverseBitsPerByte)
        }
        4 => {
            reject_extra_arguments(arguments, "invert")?;
            Ok(FilterStep::InvertBits)
        }
        5 => Ok(FilterStep::XorMask {
            mask: parse_optional_u8(arguments, "xor", 0xFF)?,
        }),
        6 => {
            let (seed, polynomial) = parse_lfsr_arguments(arguments, "scramble")?;
            Ok(FilterStep::LfsrScramble { seed, polynomial })
        }
        7 => {
            let (seed, polynomial) = parse_lfsr_arguments(arguments, "descramble")?;
            Ok(FilterStep::LfsrDescramble { seed, polynomial })
        }
        8 => {
            reject_extra_arguments(arguments, "flatten")?;
            Ok(FilterStep::Flatten)
        }
        9 => Ok(FilterStep::KeepGroupsLongerThanBytes {
            min_bytes: parse_optional_usize(arguments, "keep", 6)?,
        }),
        10 => parse_select_arguments(arguments),
        11 => Ok(FilterStep::ExtractL2Packets {
            protocol: parse_l2_protocol(arguments)?,
        }),
        _ => Err("Unknown filter command.".to_owned()),
    }
}

fn parse_select_arguments(arguments: &str) -> Result<FilterStep, String> {
    let trimmed = arguments.trim();
    if trimmed.is_empty() {
        return Ok(FilterStep::SelectBitRangeFromGroup {
            start_bit: 0,
            length_bits: 48,
        });
    }

    let Some((first_token, remainder)) = trimmed.split_once(char::is_whitespace) else {
        return Err(
            "`select` expects either `<start_bit> <length_bits>` or `<chunks>*<bits_per_chunk> <ranges>`."
                .to_owned(),
        );
    };

    if let Some((chunk_count_token, subgroup_size_token)) = first_token.split_once('*') {
        let chunk_count = parse_usize_token(chunk_count_token.trim(), "select")?;
        let subgroup_size_bits = parse_usize_token(subgroup_size_token.trim(), "select")?;
        let subgroup_ranges = parse_group_chunk_ranges(remainder, chunk_count)?;
        validate_select_subgroup_ranges(chunk_count, subgroup_size_bits, &subgroup_ranges)?;
        return Ok(FilterStep::SelectSubgroupRangesFromGroup {
            chunk_count,
            subgroup_size_bits,
            subgroup_ranges,
        });
    }

    let [start_bit, length_bits] = parse_usize_list(arguments, "select", &[0, 48])?
        .try_into()
        .expect("select should produce exactly two values");
    Ok(FilterStep::SelectBitRangeFromGroup {
        start_bit,
        length_bits,
    })
}

fn reject_extra_arguments(arguments: &str, command: &str) -> Result<(), String> {
    if arguments.trim().is_empty() {
        Ok(())
    } else {
        Err(format!("`{command}` does not take any parameters."))
    }
}

fn parse_required_positive_usize(
    arguments: &str,
    command: &str,
    default: usize,
) -> Result<usize, String> {
    let value = parse_optional_usize(arguments, command, default)?;
    if value == 0 {
        Err(format!("`{command}` requires a value greater than zero."))
    } else {
        Ok(value)
    }
}

fn parse_optional_usize(arguments: &str, command: &str, default: usize) -> Result<usize, String> {
    if arguments.trim().is_empty() {
        return Ok(default);
    }

    let values = parse_usize_list(arguments, command, &[])?;
    if values.len() != 1 {
        return Err(format!(
            "`{command}` expects exactly one numeric parameter."
        ));
    }
    Ok(values[0])
}

fn parse_optional_i64(arguments: &str, command: &str, default: i64) -> Result<i64, String> {
    if arguments.trim().is_empty() {
        return Ok(default);
    }

    let parts = split_numeric_arguments(arguments);
    if parts.len() != 1 {
        return Err(format!(
            "`{command}` expects exactly one numeric parameter."
        ));
    }
    parse_i64_token(parts[0], command)
}

fn parse_optional_u8(arguments: &str, command: &str, default: u8) -> Result<u8, String> {
    if arguments.trim().is_empty() {
        return Ok(default);
    }

    let parts = split_numeric_arguments(arguments);
    if parts.len() != 1 {
        return Err(format!(
            "`{command}` expects exactly one numeric parameter."
        ));
    }
    parse_u8_token(parts[0], command)
}

fn parse_lfsr_arguments(arguments: &str, command: &str) -> Result<(String, String), String> {
    let trimmed = arguments.trim();
    let Some((seed, polynomial)) = trimmed.split_once(char::is_whitespace) else {
        return Err(format!(
            "`{command}` expects `<seed> <polynomial>`, for example `0x7f x^7+x^3+1`."
        ));
    };

    parse_u64_token(seed.trim(), command)?;
    parse_lfsr_polynomial(polynomial.trim(), command)?;
    Ok((seed.trim().to_owned(), polynomial.trim().to_owned()))
}

fn parse_usize_list(
    arguments: &str,
    command: &str,
    default: &[usize],
) -> Result<Vec<usize>, String> {
    if arguments.trim().is_empty() {
        return Ok(default.to_vec());
    }

    split_numeric_arguments(arguments)
        .into_iter()
        .map(|part| parse_usize_token(part, command))
        .collect()
}

fn parse_group_chunk_ranges(
    arguments: &str,
    chunk_count: usize,
) -> Result<Vec<GroupChunkRange>, String> {
    let trimmed = arguments.trim();
    if trimmed.is_empty() {
        return Err("`select` expects subgroup ranges like `1-16,17-31`.".to_owned());
    }

    let mut ranges = Vec::new();
    for token in trimmed.split(',') {
        let token = token.trim();
        if token.is_empty() {
            continue;
        }

        let (start_chunk, end_chunk) = if let Some((start, end)) = token.split_once('-') {
            (
                parse_usize_token(start.trim(), "select")?,
                parse_usize_token(end.trim(), "select")?,
            )
        } else {
            let index = parse_usize_token(token, "select")?;
            (index, index)
        };

        if start_chunk > end_chunk {
            return Err(format!(
                "`select` subgroup range `{token}` must have start <= end."
            ));
        }
        if end_chunk >= chunk_count {
            return Err(format!(
                "`select` subgroup indexes are zero-based and must be smaller than {chunk_count}."
            ));
        }

        ranges.push(GroupChunkRange {
            start_chunk,
            end_chunk,
        });
    }

    if ranges.is_empty() {
        return Err("`select` expects at least one subgroup range.".to_owned());
    }

    Ok(ranges)
}

fn validate_select_subgroup_ranges(
    chunk_count: usize,
    subgroup_size_bits: usize,
    subgroup_ranges: &[GroupChunkRange],
) -> Result<(), String> {
    if chunk_count == 0 {
        return Err("`select` requires at least one subgroup.".to_owned());
    }
    if subgroup_size_bits == 0 {
        return Err("`select` requires a subgroup size greater than zero bits.".to_owned());
    }
    if subgroup_ranges.is_empty() {
        return Err("`select` requires at least one subgroup range.".to_owned());
    }

    for range in subgroup_ranges {
        if range.start_chunk > range.end_chunk {
            return Err("`select` subgroup ranges must have start <= end.".to_owned());
        }
        if range.end_chunk >= chunk_count {
            return Err(format!(
                "`select` subgroup indexes are zero-based and must be smaller than {chunk_count}."
            ));
        }
    }

    Ok(())
}

fn split_numeric_arguments(arguments: &str) -> Vec<&str> {
    arguments
        .split(|character: char| character.is_whitespace() || matches!(character, ',' | ':'))
        .filter(|part| !part.is_empty())
        .collect()
}

fn parse_usize_token(token: &str, command: &str) -> Result<usize, String> {
    if let Some(hex) = token
        .strip_prefix("0x")
        .or_else(|| token.strip_prefix("0X"))
    {
        usize::from_str_radix(hex, 16)
            .map_err(|_| format!("`{command}` expects a valid number, got `{token}`."))
    } else {
        token
            .parse::<usize>()
            .map_err(|_| format!("`{command}` expects a valid number, got `{token}`."))
    }
}

fn parse_i64_token(token: &str, command: &str) -> Result<i64, String> {
    if let Some(hex) = token
        .strip_prefix("-0x")
        .or_else(|| token.strip_prefix("-0X"))
    {
        i64::from_str_radix(hex, 16)
            .map(|value| -value)
            .map_err(|_| format!("`{command}` expects a valid number, got `{token}`."))
    } else if let Some(hex) = token
        .strip_prefix("0x")
        .or_else(|| token.strip_prefix("0X"))
    {
        i64::from_str_radix(hex, 16)
            .map_err(|_| format!("`{command}` expects a valid number, got `{token}`."))
    } else {
        token
            .parse::<i64>()
            .map_err(|_| format!("`{command}` expects a valid number, got `{token}`."))
    }
}

fn parse_u8_token(token: &str, command: &str) -> Result<u8, String> {
    if let Some(hex) = token
        .strip_prefix("0x")
        .or_else(|| token.strip_prefix("0X"))
    {
        u8::from_str_radix(hex, 16)
            .map_err(|_| format!("`{command}` expects a byte-sized number, got `{token}`."))
    } else {
        token
            .parse::<u8>()
            .map_err(|_| format!("`{command}` expects a byte-sized number, got `{token}`."))
    }
}

fn parse_u64_token(token: &str, command: &str) -> Result<u64, String> {
    if let Some(hex) = token
        .strip_prefix("0x")
        .or_else(|| token.strip_prefix("0X"))
    {
        u64::from_str_radix(hex, 16)
            .map_err(|_| format!("`{command}` expects a valid number, got `{token}`."))
    } else {
        token
            .parse::<u64>()
            .map_err(|_| format!("`{command}` expects a valid number, got `{token}`."))
    }
}

fn parse_l2_protocol(arguments: &str) -> Result<L2Protocol, String> {
    let normalized = arguments
        .trim()
        .to_ascii_lowercase()
        .replace('_', "-")
        .replace(' ', "-");

    match normalized.as_str() {
        "" | "ethernet" | "eth" => Ok(L2Protocol::Ethernet),
        "ppp" | "ppp-async" | "async-ppp" => Ok(L2Protocol::PppAsync),
        "ppp-hdlc" | "ppp-hdlc-like" | "ppp-like" => Ok(L2Protocol::PppHdlcLike),
        "hdlc" => Ok(L2Protocol::Hdlc),
        "sdlc" => Ok(L2Protocol::Sdlc),
        "cisco-hdlc" | "chdlc" => Ok(L2Protocol::CiscoHdlc),
        _ => Err(
            "Unknown L2 protocol. Try ethernet, ppp, ppp-hdlc, hdlc, sdlc, or cisco-hdlc."
                .to_owned(),
        ),
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

#[derive(Clone, Debug, PartialEq, Eq)]
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

fn chop_buffer(buffer: &BitBuffer, bits: i64) -> BitBuffer {
    let amount = usize::try_from(bits.unsigned_abs()).unwrap_or(usize::MAX);
    if bits >= 0 {
        buffer.slice_bits(amount, buffer.len_bits().saturating_sub(amount))
    } else {
        buffer.slice_bits(0, buffer.len_bits().saturating_sub(amount))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LfsrConfig {
    seed: u64,
    feedback_mask: u64,
    width: u32,
    register_mask: u64,
}

impl LfsrConfig {
    fn parse(seed: &str, polynomial: &str, command: &str) -> Result<Self, String> {
        let seed = parse_u64_token(seed.trim(), command)?;
        let parsed_polynomial = parse_lfsr_polynomial(polynomial.trim(), command)?;

        let width = bit_width(seed).max(parsed_polynomial.max_delay);
        let register_mask = low_bits_mask(width);
        Ok(Self {
            seed: seed & register_mask,
            feedback_mask: parsed_polynomial.feedback_mask & register_mask,
            width,
            register_mask,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ParsedLfsrPolynomial {
    max_delay: u32,
    feedback_mask: u64,
}

fn parse_lfsr_polynomial(polynomial: &str, command: &str) -> Result<ParsedLfsrPolynomial, String> {
    let trimmed = polynomial.trim();
    if trimmed.is_empty() {
        return Err(format!(
            "`{command}` requires a polynomial such as `x^7+x^3+1`."
        ));
    }

    if !trimmed.contains('x') && !trimmed.contains('X') {
        let numeric = parse_u64_token(trimmed, command)?;
        if numeric == 0 {
            return Err(format!("`{command}` requires a non-zero polynomial."));
        }

        return Ok(ParsedLfsrPolynomial {
            max_delay: bit_width(numeric),
            feedback_mask: numeric,
        });
    }

    let normalized = trimmed
        .replace(char::is_whitespace, "")
        .to_ascii_lowercase();
    let mut max_delay = 0u32;
    let mut feedback_mask = 0u64;
    let mut saw_delay = false;

    for term in normalized.split('+') {
        if term.is_empty() {
            return Err(format!(
                "`{command}` expects a polynomial like `x^7+x^3+1`, got `{polynomial}`."
            ));
        }

        let exponent = parse_lfsr_term(term, command, polynomial)?;
        if exponent > 0 {
            saw_delay = true;
            max_delay = max_delay.max(exponent);
        }
    }

    if !saw_delay {
        return Err(format!(
            "`{command}` requires a polynomial with degree at least 1, such as `x^7+x^3+1`."
        ));
    }

    for term in normalized.split('+') {
        let exponent = parse_lfsr_term(term, command, polynomial)?;
        if exponent == 0 {
            continue;
        }
        feedback_mask |= 1u64 << (exponent - 1);
    }

    Ok(ParsedLfsrPolynomial {
        max_delay,
        feedback_mask,
    })
}

fn parse_lfsr_term(term: &str, command: &str, original: &str) -> Result<u32, String> {
    if term == "1" {
        return Ok(0);
    }
    if term == "x" {
        return Ok(1);
    }
    if let Some(exponent) = term.strip_prefix("x^") {
        let exponent = exponent.parse::<u32>().map_err(|_| {
            format!("`{command}` expects a polynomial like `x^7+x^3+1`, got `{original}`.")
        })?;
        if exponent > 63 {
            return Err(format!(
                "`{command}` supports polynomial exponents up to 63, got `x^{exponent}`."
            ));
        }
        return Ok(exponent);
    }

    Err(format!(
        "`{command}` expects a polynomial like `x^7+x^3+1`, got `{original}`."
    ))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LfsrMode {
    Scramble,
    Descramble,
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn build_derived_view(bytes: &[u8], pipeline: &FilterPipeline) -> Result<DerivedView, String> {
    build_cached_filter_state(bytes, pipeline).map(|state| state.to_derived_view())
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn build_derived_view_from_groups(
    groups: &[Vec<u8>],
    pipeline: &FilterPipeline,
) -> Result<DerivedView, String> {
    build_cached_filter_state_from_groups(groups, pipeline).map(|state| state.to_derived_view())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CachedFilterState {
    state: PipelineState,
}

impl CachedFilterState {
    pub fn to_derived_view(&self) -> DerivedView {
        build_derived_view_from_state(self.state.clone())
    }
}

pub fn build_cached_filter_state(
    bytes: &[u8],
    pipeline: &FilterPipeline,
) -> Result<CachedFilterState, String> {
    build_cached_filter_state_from_state(
        PipelineState::Flat(BitBuffer::from_bytes(bytes.to_vec())),
        pipeline,
    )
}

pub fn build_cached_filter_state_from_groups(
    groups: &[Vec<u8>],
    pipeline: &FilterPipeline,
) -> Result<CachedFilterState, String> {
    let state = PipelineState::Grouped(groups.iter().cloned().map(BitBuffer::from_bytes).collect());
    build_cached_filter_state_from_state(state, pipeline)
}

pub fn append_filter_to_cached_state(
    cached_state: &CachedFilterState,
    step: &FilterStep,
) -> Result<CachedFilterState, String> {
    Ok(CachedFilterState {
        state: apply_step(cached_state.state.clone(), step)?,
    })
}

fn build_cached_filter_state_from_state(
    mut state: PipelineState,
    pipeline: &FilterPipeline,
) -> Result<CachedFilterState, String> {
    for step in &pipeline.steps {
        state = apply_step(state, step)?;
    }

    Ok(CachedFilterState { state })
}

fn build_derived_view_from_state(state: PipelineState) -> DerivedView {
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

    DerivedView::new(groups)
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
            PipelineState::Flat(buffer) => Ok(PipelineState::Flat(chop_buffer(&buffer, *bits))),
            PipelineState::Grouped(groups) => Ok(PipelineState::Grouped(
                groups
                    .into_iter()
                    .map(|group| chop_buffer(&group, *bits))
                    .filter(|group| !group.is_empty())
                    .collect(),
            )),
        },
        FilterStep::ReverseBitsPerByte => Ok(state.map_bytes(u8::reverse_bits)),
        FilterStep::InvertBits => Ok(state.map_bytes(|byte| !byte)),
        FilterStep::XorMask { mask } => Ok(state.map_bytes(|byte| byte ^ mask)),
        FilterStep::LfsrScramble { seed, polynomial } => {
            let config = LfsrConfig::parse(seed, polynomial, "scramble")?;
            Ok(apply_lfsr(state, config, LfsrMode::Scramble))
        }
        FilterStep::LfsrDescramble { seed, polynomial } => {
            let config = LfsrConfig::parse(seed, polynomial, "descramble")?;
            Ok(apply_lfsr(state, config, LfsrMode::Descramble))
        }
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
        FilterStep::SelectSubgroupRangesFromGroup {
            chunk_count,
            subgroup_size_bits,
            subgroup_ranges,
        } => match state {
            PipelineState::Flat(_) => Err(
                "Select-range filter requires a grouping step earlier in the pipeline.".to_owned(),
            ),
            PipelineState::Grouped(groups) => {
                validate_select_subgroup_ranges(
                    *chunk_count,
                    *subgroup_size_bits,
                    subgroup_ranges,
                )?;
                Ok(PipelineState::Grouped(
                    groups
                        .into_iter()
                        .map(|group| {
                            let selected_chunks = subgroup_ranges
                                .iter()
                                .flat_map(|range| range.start_chunk..=range.end_chunk)
                                .map(|chunk_index| {
                                    group.slice_bits(
                                        chunk_index.saturating_mul(*subgroup_size_bits),
                                        *subgroup_size_bits,
                                    )
                                })
                                .collect::<Vec<_>>();
                            BitBuffer::concatenate(&selected_chunks)
                        })
                        .filter(|group| !group.is_empty())
                        .collect(),
                ))
            }
        },
        FilterStep::ExtractL2Packets { protocol } => extract_l2_packets(state, *protocol),
    }
}

fn apply_lfsr(state: PipelineState, config: LfsrConfig, mode: LfsrMode) -> PipelineState {
    match state {
        PipelineState::Flat(buffer) => {
            PipelineState::Flat(apply_lfsr_to_buffer(&buffer, config, mode))
        }
        PipelineState::Grouped(groups) => PipelineState::Grouped(
            groups
                .into_iter()
                .map(|group| apply_lfsr_to_buffer(&group, config, mode))
                .collect(),
        ),
    }
}

fn apply_lfsr_to_buffer(buffer: &BitBuffer, config: LfsrConfig, mode: LfsrMode) -> BitBuffer {
    let mut register = config.seed & config.register_mask;
    let mut output = BitBuffer::default();

    for bit_index in 0..buffer.len_bits() {
        let input_bit = buffer.bit(bit_index).unwrap_or(0);
        let feedback_bit = parity_bit(register & config.feedback_mask);
        let output_bit = input_bit ^ feedback_bit;
        output.push_bit(output_bit);

        let shifted = register << 1;
        let history_bit = match mode {
            LfsrMode::Scramble => output_bit,
            LfsrMode::Descramble => input_bit,
        } as u64;
        register = if config.width == u64::BITS {
            shifted | history_bit
        } else {
            (shifted | history_bit) & config.register_mask
        };
    }

    output
}

fn parity_bit(value: u64) -> u8 {
    (value.count_ones() & 1) as u8
}

fn bit_width(value: u64) -> u32 {
    u64::BITS - value.leading_zeros()
}

fn low_bits_mask(width: u32) -> u64 {
    if width >= u64::BITS {
        u64::MAX
    } else {
        (1u64 << width) - 1
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
        DerivedView, FilterPipeline, FilterStep, GroupChunkRange, L2Protocol,
        append_filter_to_cached_state, build_cached_filter_state, build_derived_view,
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
    fn select_subgroup_ranges_concatenates_requested_virtual_chunks() {
        let bytes = [0b1111_0000, 0b1010_0101];
        let pipeline = FilterPipeline {
            steps: vec![
                FilterStep::Split {
                    group_size_bits: 16,
                },
                FilterStep::SelectSubgroupRangesFromGroup {
                    chunk_count: 4,
                    subgroup_size_bits: 4,
                    subgroup_ranges: vec![
                        GroupChunkRange {
                            start_chunk: 1,
                            end_chunk: 2,
                        },
                        GroupChunkRange {
                            start_chunk: 3,
                            end_chunk: 3,
                        },
                    ],
                },
            ],
        };

        let view = build_derived_view(&bytes, &pipeline).expect("pipeline should succeed");

        assert_eq!(
            group_bits(&view),
            vec![vec![0, 0, 0, 0, 1, 0, 1, 0, 0, 1, 0, 1]]
        );
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
    fn chop_with_negative_value_removes_suffix_once_when_input_is_still_flat() {
        let bytes = [0b1101_0011];
        let pipeline = FilterPipeline {
            steps: vec![FilterStep::Chop { bits: -3 }],
        };

        let view = build_derived_view(&bytes, &pipeline).expect("pipeline should succeed");

        assert_eq!(group_bits(&view), vec![vec![1, 1, 0, 1, 0]]);
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
    fn chop_with_negative_value_removes_suffix_from_each_existing_group() {
        let groups = vec![vec![0xAA], vec![0x55, 0xF0]];
        let pipeline = FilterPipeline {
            steps: vec![FilterStep::Chop { bits: -4 }],
        };

        let view = super::build_derived_view_from_groups(&groups, &pipeline)
            .expect("pipeline should succeed");

        assert_eq!(
            group_bits(&view),
            vec![vec![1, 0, 1, 0], vec![0, 1, 0, 1, 0, 1, 0, 1, 1, 1, 1, 1],]
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
    fn select_subgroup_ranges_rejects_invalid_indexes() {
        let bytes = [0b1111_0000, 0b1010_0101];
        let pipeline = FilterPipeline {
            steps: vec![
                FilterStep::Split {
                    group_size_bits: 16,
                },
                FilterStep::SelectSubgroupRangesFromGroup {
                    chunk_count: 4,
                    subgroup_size_bits: 4,
                    subgroup_ranges: vec![GroupChunkRange {
                        start_chunk: 1,
                        end_chunk: 4,
                    }],
                },
            ],
        };

        let error = build_derived_view(&bytes, &pipeline).expect_err("pipeline should fail");
        assert!(error.contains("zero-based"));
    }

    #[test]
    fn append_filter_to_cached_state_matches_full_rebuild() {
        let bytes = [0b1010_0001, 0b1010_1111, 0b1010_0010];
        let base_pipeline = FilterPipeline {
            steps: vec![FilterStep::SyncOnPreamble {
                bits: "1010".to_owned(),
            }],
        };
        let appended_step = FilterStep::SelectBitRangeFromGroup {
            start_bit: 0,
            length_bits: 4,
        };
        let full_pipeline = FilterPipeline {
            steps: vec![base_pipeline.steps[0].clone(), appended_step.clone()],
        };

        let cached_state =
            build_cached_filter_state(&bytes, &base_pipeline).expect("base pipeline should work");
        let appended_state = append_filter_to_cached_state(&cached_state, &appended_step)
            .expect("appending the final step should work");
        let full_state =
            build_cached_filter_state(&bytes, &full_pipeline).expect("full pipeline should work");

        assert_eq!(appended_state, full_state);
        assert_eq!(
            appended_state.to_derived_view(),
            build_derived_view(&bytes, &full_pipeline).expect("full view should build")
        );
    }

    #[test]
    fn cached_state_preserves_flat_state_for_group_only_filters() {
        let bytes = [0b1111_0000];
        let cached_state =
            build_cached_filter_state(&bytes, &FilterPipeline::default()).expect("base view");
        let error = append_filter_to_cached_state(
            &cached_state,
            &FilterStep::KeepGroupsLongerThanBytes { min_bytes: 1 },
        )
        .expect_err("group-only filter should still reject flat cached input");

        assert!(error.contains("requires a grouping step"));
    }

    #[test]
    fn parse_preamble_bits_accepts_hex_input() {
        let bits = parse_preamble_bits("0xA5").expect("hex preamble should parse");

        assert_eq!(bits, vec![1, 0, 1, 0, 0, 1, 0, 1]);
    }

    #[test]
    fn parse_filter_command_accepts_parameters_and_defaults() {
        assert_eq!(
            super::parse_filter_command("split 256").expect("split should parse"),
            FilterStep::Split {
                group_size_bits: 256,
            }
        );
        assert_eq!(
            super::parse_filter_command("chop").expect("chop should use its default"),
            FilterStep::Chop { bits: 8 }
        );
        assert_eq!(
            super::parse_filter_command("chop -8").expect("negative chop should parse"),
            FilterStep::Chop { bits: -8 }
        );
        assert_eq!(
            super::parse_filter_command("select 12, 24").expect("select should parse two values"),
            FilterStep::SelectBitRangeFromGroup {
                start_bit: 12,
                length_bits: 24,
            }
        );
        assert_eq!(
            super::parse_filter_command("select 32*8 1-16,17-31")
                .expect("chunked select should parse"),
            FilterStep::SelectSubgroupRangesFromGroup {
                chunk_count: 32,
                subgroup_size_bits: 8,
                subgroup_ranges: vec![
                    GroupChunkRange {
                        start_chunk: 1,
                        end_chunk: 16,
                    },
                    GroupChunkRange {
                        start_chunk: 17,
                        end_chunk: 31,
                    },
                ],
            }
        );
    }

    #[test]
    fn parse_filter_command_accepts_protocol_and_mask_aliases() {
        assert_eq!(
            super::parse_filter_command("xor 0xaa").expect("xor should parse hex masks"),
            FilterStep::XorMask { mask: 0xAA }
        );
        assert_eq!(
            super::parse_filter_command("scramble 0x7f x^7+x^3+1")
                .expect("scramble should parse polynomial notation"),
            FilterStep::LfsrScramble {
                seed: "0x7f".to_owned(),
                polynomial: "x^7+x^3+1".to_owned(),
            }
        );
        assert_eq!(
            super::parse_filter_command("descramble 127 x^7 + x^3 + 1")
                .expect("descramble should parse spaced polynomial notation"),
            FilterStep::LfsrDescramble {
                seed: "127".to_owned(),
                polynomial: "x^7 + x^3 + 1".to_owned(),
            }
        );
        assert_eq!(
            super::parse_filter_command("extract cisco hdlc")
                .expect("extract should parse protocol aliases"),
            FilterStep::ExtractL2Packets {
                protocol: L2Protocol::CiscoHdlc,
            }
        );
    }

    #[test]
    fn complete_filter_command_completes_unique_prefixes() {
        assert_eq!(
            super::complete_filter_command("sp"),
            Some("split ".to_owned())
        );
        assert_eq!(
            super::complete_filter_command("pre"),
            Some("sync ".to_owned())
        );
        assert_eq!(super::complete_filter_command("s"), None);
        assert_eq!(super::complete_filter_command("split 256"), None);
    }

    #[test]
    fn parse_filter_command_rejects_unknown_commands() {
        let error =
            super::parse_filter_command("unknown 1").expect_err("unknown filter should fail");

        assert!(error.contains("Unknown filter command"));
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
    fn lfsr_scramble_and_descramble_round_trip_flat_input() {
        let bytes = [0xA5, 0x5A, 0xFF, 0x00];
        let pipeline = FilterPipeline {
            steps: vec![
                FilterStep::LfsrScramble {
                    seed: "0x7f".to_owned(),
                    polynomial: "x^7+x^3+1".to_owned(),
                },
                FilterStep::LfsrDescramble {
                    seed: "0x7f".to_owned(),
                    polynomial: "x^7+x^3+1".to_owned(),
                },
            ],
        };

        let view = build_derived_view(&bytes, &pipeline).expect("LFSR round trip should succeed");
        assert_eq!(group_bytes(&view), vec![bytes.to_vec()]);
    }

    #[test]
    fn lfsr_scramble_and_descramble_round_trip_grouped_input() {
        let groups = vec![vec![0xAA, 0x55], vec![0xF0], vec![0x0F, 0x33, 0xCC]];
        let pipeline = FilterPipeline {
            steps: vec![
                FilterStep::LfsrScramble {
                    seed: "0x15".to_owned(),
                    polynomial: "x^4+1".to_owned(),
                },
                FilterStep::LfsrDescramble {
                    seed: "0x15".to_owned(),
                    polynomial: "x^4+1".to_owned(),
                },
            ],
        };

        let view = super::build_derived_view_from_groups(&groups, &pipeline)
            .expect("grouped LFSR round trip should succeed");
        assert_eq!(group_bytes(&view), groups);
    }

    #[test]
    fn etsi_polynomial_matches_published_zero_input_recursion() {
        let bytes = [0u8; 16];
        let pipeline = FilterPipeline {
            steps: vec![FilterStep::LfsrScramble {
                seed: "1".to_owned(),
                polynomial: "x^23+x^18+1".to_owned(),
            }],
        };

        let view = build_derived_view(&bytes, &pipeline).expect("scrambler should build");
        let bits = group_bits(&view)
            .pop()
            .expect("flat input should yield one group");

        for index in 23..bits.len() {
            assert_eq!(
                bits[index],
                bits[index - 18] ^ bits[index - 23],
                "ETSI recursion failed at bit {index}"
            );
        }
    }

    #[test]
    fn intel_10gbase_r_descrambler_self_synchronizes_after_58_bits() {
        let bytes = [
            0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0, 0x55, 0xAA, 0x11, 0x22, 0x33, 0x44,
            0x66, 0x77, 0x88, 0x99, 0xAB, 0xCD, 0xEF, 0x01, 0x23, 0x45,
        ];
        let pipeline = FilterPipeline {
            steps: vec![
                FilterStep::LfsrScramble {
                    seed: "0x3ffffffffffffff".to_owned(),
                    polynomial: "x^58+x^39+1".to_owned(),
                },
                FilterStep::LfsrDescramble {
                    seed: "1".to_owned(),
                    polynomial: "x^58+x^39+1".to_owned(),
                },
            ],
        };

        let view = build_derived_view(&bytes, &pipeline).expect("pipeline should succeed");
        let bits = group_bits(&view)
            .pop()
            .expect("flat input should yield one group");
        let original_bits = bytes_to_bits(&bytes);

        assert_eq!(&bits[58..], &original_bits[58..]);
    }

    #[test]
    fn ppp_over_sonet_single_scrambled_bit_error_becomes_two_descrambled_bits() {
        let scrambled_error = bits_to_bytes(&[
            1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0,
        ]);
        let pipeline = FilterPipeline {
            steps: vec![FilterStep::LfsrDescramble {
                seed: "0".to_owned(),
                polynomial: "x^43+1".to_owned(),
            }],
        };

        let view =
            build_derived_view(&scrambled_error, &pipeline).expect("descrambler should build");
        let bits = group_bits(&view)
            .pop()
            .expect("flat input should yield one group");
        let errored_positions = bits
            .iter()
            .enumerate()
            .filter_map(|(index, bit)| (*bit != 0).then_some(index))
            .collect::<Vec<_>>();

        assert_eq!(errored_positions, vec![0, 43]);
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
