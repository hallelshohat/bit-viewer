use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;
use std::time::Duration;

use pcap_file::DataLink;
use pcap_file::pcap::{PcapHeader, PcapPacket, PcapWriter};

use crate::filters::DerivedView;

const MAX_KNOWN_LINK_TYPE_ID: u32 = 296;
const WAVE_FORMAT_PCM: u16 = 0x0001;
const WAVE_FORMAT_IEEE_FLOAT: u16 = 0x0003;
const WAVE_FORMAT_ALAW: u16 = 0x0006;
const WAVE_FORMAT_MULAW: u16 = 0x0007;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportFormat {
    FlattenedBits,
    Pcap,
    Wav,
}

impl ExportFormat {
    pub fn default_extension(self) -> &'static str {
        match self {
            Self::FlattenedBits => "bin",
            Self::Pcap => "pcap",
            Self::Wav => "wav",
        }
    }

    pub fn filter_label(self) -> &'static str {
        match self {
            Self::FlattenedBits => "Binary files",
            Self::Pcap => "PCAP files",
            Self::Wav => "WAV files",
        }
    }

    pub fn success_label(self) -> &'static str {
        match self {
            Self::FlattenedBits => "Flattened bit export",
            Self::Pcap => "PCAP export",
            Self::Wav => "WAV export",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkTypeOption {
    pub id: u32,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PcapExportOptions {
    pub link_type: u32,
    pub timestamp_step_micros: u32,
}

impl Default for PcapExportOptions {
    fn default() -> Self {
        Self {
            link_type: 1,
            timestamp_step_micros: 1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WavCodec {
    PcmUnsigned8,
    PcmSigned16,
    PcmSigned24,
    PcmSigned32,
    Float32,
    Float64,
    ALaw,
    MuLaw,
}

impl WavCodec {
    pub fn label(self) -> &'static str {
        match self {
            Self::PcmUnsigned8 => "PCM unsigned 8-bit",
            Self::PcmSigned16 => "PCM signed 16-bit",
            Self::PcmSigned24 => "PCM signed 24-bit",
            Self::PcmSigned32 => "PCM signed 32-bit",
            Self::Float32 => "IEEE float 32-bit",
            Self::Float64 => "IEEE float 64-bit",
            Self::ALaw => "A-LAW 8-bit",
            Self::MuLaw => "mu-LAW 8-bit",
        }
    }

    pub fn bytes_per_sample(self) -> u16 {
        match self {
            Self::PcmUnsigned8 | Self::ALaw | Self::MuLaw => 1,
            Self::PcmSigned16 => 2,
            Self::PcmSigned24 => 3,
            Self::PcmSigned32 | Self::Float32 => 4,
            Self::Float64 => 8,
        }
    }

    pub fn bits_per_sample(self) -> u16 {
        self.bytes_per_sample() * 8
    }

    fn format_tag(self) -> u16 {
        match self {
            Self::PcmUnsigned8 | Self::PcmSigned16 | Self::PcmSigned24 | Self::PcmSigned32 => {
                WAVE_FORMAT_PCM
            }
            Self::Float32 | Self::Float64 => WAVE_FORMAT_IEEE_FLOAT,
            Self::ALaw => WAVE_FORMAT_ALAW,
            Self::MuLaw => WAVE_FORMAT_MULAW,
        }
    }

    fn fmt_chunk_size(self) -> u32 {
        match self {
            Self::ALaw | Self::MuLaw => 18,
            _ => 16,
        }
    }
}

pub const WAV_CODEC_PRESETS: [WavCodec; 8] = [
    WavCodec::PcmUnsigned8,
    WavCodec::PcmSigned16,
    WavCodec::PcmSigned24,
    WavCodec::PcmSigned32,
    WavCodec::Float32,
    WavCodec::Float64,
    WavCodec::ALaw,
    WavCodec::MuLaw,
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WavExportOptions {
    pub codec: WavCodec,
    pub sample_rate: u32,
    pub channels: u16,
}

impl Default for WavExportOptions {
    fn default() -> Self {
        Self {
            codec: WavCodec::PcmUnsigned8,
            sample_rate: 44_100,
            channels: 1,
        }
    }
}

pub fn known_link_types() -> Vec<LinkTypeOption> {
    let mut options = Vec::with_capacity((MAX_KNOWN_LINK_TYPE_ID + 1) as usize);
    for id in 0..=MAX_KNOWN_LINK_TYPE_ID {
        let link = DataLink::from(id);
        if matches!(link, DataLink::Unknown(_)) {
            continue;
        }

        options.push(LinkTypeOption {
            id,
            label: link_type_label(link),
        });
    }

    options
}

pub fn export_flattened_bits(view: &DerivedView, path: &Path) -> Result<(), String> {
    let file =
        File::create(path).map_err(|error| format!("Failed to create export file: {error}"))?;
    let mut writer = BufWriter::new(file);
    write_flattened_bits(&mut writer, view)?;
    writer
        .flush()
        .map_err(|error| format!("Failed to flush export file: {error}"))
}

pub fn export_pcap(
    view: &DerivedView,
    path: &Path,
    options: &PcapExportOptions,
) -> Result<(), String> {
    let file =
        File::create(path).map_err(|error| format!("Failed to create export file: {error}"))?;
    let writer = BufWriter::new(file);
    write_pcap(writer, view, options)
}

pub fn export_wav(
    view: &DerivedView,
    path: &Path,
    options: &WavExportOptions,
) -> Result<(), String> {
    let file =
        File::create(path).map_err(|error| format!("Failed to create export file: {error}"))?;
    let mut writer = BufWriter::new(file);
    write_wav(&mut writer, view, options)?;
    writer
        .flush()
        .map_err(|error| format!("Failed to flush export file: {error}"))
}

pub fn write_flattened_bits<W: Write>(writer: &mut W, view: &DerivedView) -> Result<(), String> {
    let bytes = view.flattened_packed_bytes();
    writer
        .write_all(&bytes)
        .map_err(|error| format!("Failed to write flattened bit export: {error}"))
}

pub fn write_pcap<W: Write>(
    writer: W,
    view: &DerivedView,
    options: &PcapExportOptions,
) -> Result<(), String> {
    let snaplen = view
        .groups()
        .iter()
        .map(|group| group.len_bytes_rounded_up())
        .max()
        .unwrap_or(0)
        .try_into()
        .map_err(|_| "Largest group does not fit into PCAP snaplen.".to_owned())?;

    let header = PcapHeader {
        datalink: DataLink::from(options.link_type),
        snaplen,
        ..Default::default()
    };

    let mut pcap_writer = PcapWriter::with_header(writer, header)
        .map_err(|error| format!("Failed to start PCAP export: {error}"))?;

    let step = u64::from(options.timestamp_step_micros.max(1));
    for (index, group) in view.groups().iter().enumerate() {
        let packet_bytes = group.packed_bytes().to_vec();
        let packet_len = packet_bytes
            .len()
            .try_into()
            .map_err(|_| "A packet is too large to write into PCAP.".to_owned())?;
        let timestamp = Duration::from_micros(index as u64 * step);
        let packet = PcapPacket::new_owned(timestamp, packet_len, packet_bytes);
        pcap_writer
            .write_packet(&packet)
            .map_err(|error| format!("Failed to write PCAP packet: {error}"))?;
    }

    pcap_writer
        .flush()
        .map_err(|error| format!("Failed to flush PCAP export: {error}"))
}

pub fn write_wav<W: Write>(
    writer: &mut W,
    view: &DerivedView,
    options: &WavExportOptions,
) -> Result<(), String> {
    if options.channels == 0 {
        return Err("WAV channel count must be at least 1.".to_owned());
    }

    if options.sample_rate == 0 {
        return Err("WAV sample rate must be at least 1 Hz.".to_owned());
    }

    let data = view.flattened_packed_bytes();
    let bytes_per_frame =
        usize::from(options.codec.bytes_per_sample()).saturating_mul(usize::from(options.channels));
    if bytes_per_frame == 0 {
        return Err("WAV export has an invalid frame size.".to_owned());
    }

    if data.len() % bytes_per_frame != 0 {
        return Err(format!(
            "Flattened data length ({}) is not aligned to the selected WAV frame size ({} bytes).",
            data.len(),
            bytes_per_frame
        ));
    }

    write_wav_header(writer, options, data.len())?;
    writer
        .write_all(&data)
        .map_err(|error| format!("Failed to write WAV data: {error}"))
}

pub fn default_export_file_name(source_file_name: &str, format: ExportFormat) -> String {
    let stem = Path::new(source_file_name)
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("export");

    match format {
        ExportFormat::FlattenedBits => format!("{stem}-flattened.{}", format.default_extension()),
        ExportFormat::Pcap => format!("{stem}.{}", format.default_extension()),
        ExportFormat::Wav => format!("{stem}.{}", format.default_extension()),
    }
}

fn write_wav_header<W: Write>(
    writer: &mut W,
    options: &WavExportOptions,
    data_len: usize,
) -> Result<(), String> {
    let data_len_u32 = u32::try_from(data_len)
        .map_err(|_| "WAV export is too large for a RIFF data chunk.".to_owned())?;
    let fmt_chunk_size = options.codec.fmt_chunk_size();
    let riff_size = 4u32
        .saturating_add(8 + fmt_chunk_size)
        .saturating_add(8 + data_len_u32);
    let bytes_per_sample = u32::from(options.codec.bytes_per_sample());
    let channels = u32::from(options.channels);
    let block_align = u16::try_from(channels.saturating_mul(bytes_per_sample))
        .map_err(|_| "WAV block alignment does not fit into the header.".to_owned())?;
    let byte_rate = options
        .sample_rate
        .checked_mul(u32::from(block_align))
        .ok_or_else(|| "WAV byte rate overflowed the header field.".to_owned())?;

    writer
        .write_all(b"RIFF")
        .and_then(|_| writer.write_all(&riff_size.to_le_bytes()))
        .and_then(|_| writer.write_all(b"WAVE"))
        .and_then(|_| writer.write_all(b"fmt "))
        .and_then(|_| writer.write_all(&fmt_chunk_size.to_le_bytes()))
        .and_then(|_| writer.write_all(&options.codec.format_tag().to_le_bytes()))
        .and_then(|_| writer.write_all(&options.channels.to_le_bytes()))
        .and_then(|_| writer.write_all(&options.sample_rate.to_le_bytes()))
        .and_then(|_| writer.write_all(&byte_rate.to_le_bytes()))
        .and_then(|_| writer.write_all(&block_align.to_le_bytes()))
        .and_then(|_| writer.write_all(&options.codec.bits_per_sample().to_le_bytes()))
        .map_err(|error| format!("Failed to write WAV header: {error}"))?;

    if matches!(options.codec, WavCodec::ALaw | WavCodec::MuLaw) {
        writer
            .write_all(&0u16.to_le_bytes())
            .map_err(|error| format!("Failed to write WAV codec extension: {error}"))?;
    }

    writer
        .write_all(b"data")
        .and_then(|_| writer.write_all(&data_len_u32.to_le_bytes()))
        .map_err(|error| format!("Failed to write WAV data header: {error}"))
}

fn link_type_label(link: DataLink) -> String {
    match link {
        DataLink::Unknown(id) => format!("Unknown ({id})"),
        _ => format!("{link:?}"),
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use pcap_file::pcap::PcapReader;

    use crate::filters::{FilterPipeline, FilterStep, build_derived_view};

    use super::{
        PcapExportOptions, WAV_CODEC_PRESETS, WavCodec, WavExportOptions, known_link_types,
        write_flattened_bits, write_pcap, write_wav,
    };

    #[test]
    fn flattened_export_pads_partial_tail_byte() {
        let pipeline = FilterPipeline {
            steps: vec![
                FilterStep::SyncOnPreamble {
                    bits: "111".to_owned(),
                },
                FilterStep::SelectBitRangeFromGroup {
                    start_bit: 0,
                    length_bits: 3,
                },
                FilterStep::Flatten,
            ],
        };
        let view = build_derived_view(&[0b1110_0000], &pipeline).expect("pipeline should succeed");
        let mut output = Vec::new();

        write_flattened_bits(&mut output, &view).expect("flattened export should succeed");

        assert_eq!(output, vec![0b1110_0000]);
    }

    #[test]
    fn pcap_export_writes_one_packet_per_group() {
        let pipeline = FilterPipeline {
            steps: vec![FilterStep::SyncOnPreamble {
                bits: "1010".to_owned(),
            }],
        };
        let view = build_derived_view(&[0b1010_0001, 0b1010_1111], &pipeline)
            .expect("pipeline should succeed");
        let mut output = Cursor::new(Vec::new());

        write_pcap(
            &mut output,
            &view,
            &PcapExportOptions {
                link_type: 105,
                timestamp_step_micros: 10,
            },
        )
        .expect("pcap export should succeed");

        let bytes = output.into_inner();
        let mut reader = PcapReader::new(Cursor::new(bytes)).expect("pcap should parse");
        assert_eq!(u32::from(reader.header().datalink), 105);
        assert!(reader.next_packet().is_some());
        assert!(reader.next_packet().is_some());
        assert!(reader.next_packet().is_none());
    }

    #[test]
    fn wav_export_writes_mulaw_header() {
        let view = build_derived_view(&[0x12, 0x34, 0x56, 0x78], &Default::default())
            .expect("view should build");
        let mut output = Vec::new();

        write_wav(
            &mut output,
            &view,
            &WavExportOptions {
                codec: WavCodec::MuLaw,
                sample_rate: 8_000,
                channels: 1,
            },
        )
        .expect("wav export should succeed");

        assert_eq!(&output[0..4], b"RIFF");
        assert_eq!(&output[8..12], b"WAVE");
        assert_eq!(&output[12..16], b"fmt ");
        assert_eq!(u16::from_le_bytes([output[20], output[21]]), 0x0007);
        assert_eq!(&output[38..42], b"data");
        assert_eq!(&output[42..46], &(4u32).to_le_bytes());
    }

    #[test]
    fn wav_export_rejects_unaligned_frames() {
        let view = build_derived_view(&[0x12, 0x34, 0x56], &Default::default())
            .expect("view should build");
        let mut output = Vec::new();
        let error = write_wav(
            &mut output,
            &view,
            &WavExportOptions {
                codec: WavCodec::PcmSigned16,
                sample_rate: 44_100,
                channels: 1,
            },
        )
        .expect_err("wav export should fail");

        assert!(error.contains("frame size"));
    }

    #[test]
    fn known_link_types_cover_common_values() {
        let link_types = known_link_types();

        assert!(
            link_types
                .iter()
                .any(|item| item.id == 1 && item.label == "ETHERNET")
        );
        assert!(
            link_types
                .iter()
                .any(|item| item.id == 105 && item.label == "IEEE802_11")
        );
        assert!(
            link_types
                .iter()
                .any(|item| item.id == 276 && item.label == "LINUX_SLL2")
        );
        assert_eq!(WAV_CODEC_PRESETS.len(), 8);
    }
}
