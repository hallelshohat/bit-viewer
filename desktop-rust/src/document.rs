use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use memmap2::{Mmap, MmapOptions};
use pcap_file::pcap::PcapReader;

use crate::filters::{
    CachedFilterState, DerivedView, FilterPipeline, build_cached_filter_state,
    build_cached_filter_state_from_groups, build_derived_view, build_derived_view_from_groups,
};

const PCAP_MAGIC_NUMBERS: [[u8; 4]; 4] = [
    [0xD4, 0xC3, 0xB2, 0xA1],
    [0xA1, 0xB2, 0xC3, 0xD4],
    [0x4D, 0x3C, 0xB2, 0xA1],
    [0xA1, 0xB2, 0x3C, 0x4D],
];

pub struct BinaryDocument {
    path: PathBuf,
    file_name: String,
    source: DocumentSource,
}

enum DocumentSource {
    Bytes(Mmap),
    PacketGroups(Vec<Vec<u8>>),
}

impl BinaryDocument {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref().to_path_buf();
        let mut file =
            File::open(&path).map_err(|error| format!("Failed to open file: {error}"))?;
        let file_name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("unnamed")
            .to_owned();

        let source = if should_import_as_pcap(&path, &mut file)? {
            DocumentSource::PacketGroups(read_pcap_packet_groups(file)?)
        } else {
            let mmap = unsafe { MmapOptions::new().map(&file) }
                .map_err(|error| format!("Failed to memory-map file: {error}"))?;
            DocumentSource::Bytes(mmap)
        };

        Ok(Self {
            path,
            file_name,
            source,
        })
    }

    pub fn file_name(&self) -> &str {
        &self.file_name
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn len_bytes(&self) -> usize {
        match &self.source {
            DocumentSource::Bytes(mmap) => mmap.len(),
            DocumentSource::PacketGroups(groups) => groups.iter().map(Vec::len).sum(),
        }
    }

    pub fn len_bits(&self) -> usize {
        self.len_bytes().saturating_mul(8)
    }

    pub fn source_size_label(&self) -> String {
        match &self.source {
            DocumentSource::Bytes(_) => format!("source {} bytes", self.len_bytes()),
            DocumentSource::PacketGroups(_) => format!("packet payload {} bytes", self.len_bytes()),
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn build_derived_view(&self, pipeline: &FilterPipeline) -> Result<DerivedView, String> {
        match &self.source {
            DocumentSource::Bytes(mmap) => build_derived_view(mmap, pipeline),
            DocumentSource::PacketGroups(groups) => {
                build_derived_view_from_groups(groups, pipeline)
            }
        }
    }

    pub fn build_cached_filter_state(
        &self,
        pipeline: &FilterPipeline,
    ) -> Result<CachedFilterState, String> {
        match &self.source {
            DocumentSource::Bytes(mmap) => build_cached_filter_state(mmap, pipeline),
            DocumentSource::PacketGroups(groups) => {
                build_cached_filter_state_from_groups(groups, pipeline)
            }
        }
    }
}

fn should_import_as_pcap(path: &Path, file: &mut File) -> Result<bool, String> {
    if path
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("pcap"))
    {
        return Ok(true);
    }

    let mut magic = [0u8; 4];
    let bytes_read = file
        .read(&mut magic)
        .map_err(|error| format!("Failed to inspect file header: {error}"))?;
    file.seek(SeekFrom::Start(0))
        .map_err(|error| format!("Failed to rewind file after header read: {error}"))?;

    Ok(bytes_read == magic.len() && PCAP_MAGIC_NUMBERS.contains(&magic))
}

fn read_pcap_packet_groups(file: File) -> Result<Vec<Vec<u8>>, String> {
    let mut reader =
        PcapReader::new(file).map_err(|error| format!("Failed to parse PCAP file: {error}"))?;
    let mut groups = Vec::new();

    while let Some(packet) = reader.next_packet() {
        let packet =
            packet.map_err(|error| format!("Failed to read PCAP packet payload: {error}"))?;
        groups.push(packet.data.into_owned());
    }

    Ok(groups)
}

#[cfg(test)]
mod tests {
    use super::BinaryDocument;
    use std::env;
    use std::fs::{self, File};
    use std::path::{Path, PathBuf};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use pcap_file::DataLink;
    use pcap_file::pcap::{PcapHeader, PcapPacket, PcapWriter};

    use crate::filters::FilterPipeline;

    #[test]
    fn open_pcap_imports_packet_payloads_as_groups() {
        let path = unique_temp_path("packets-as-groups.pcap");
        write_test_pcap(&path, &[b"\x01\x02", b"\xAA\xBB\xCC"]);

        let document = BinaryDocument::open(&path).expect("pcap should load");
        let view = document
            .build_derived_view(&FilterPipeline::default())
            .expect("view should build");

        assert_eq!(document.len_bytes(), 5);
        assert_eq!(document.source_size_label(), "packet payload 5 bytes");
        assert_eq!(view.group_count(), 2);
        assert_eq!(view.groups()[0].packed_bytes(), &[0x01, 0x02]);
        assert_eq!(view.groups()[1].packed_bytes(), &[0xAA, 0xBB, 0xCC]);

        let _ = fs::remove_file(path);
    }

    fn write_test_pcap(path: &Path, packets: &[&[u8]]) {
        let file = File::create(path).expect("temp pcap should be creatable");
        let header = PcapHeader {
            datalink: DataLink::ETHERNET,
            snaplen: 65_535,
            ..Default::default()
        };
        let mut writer = PcapWriter::with_header(file, header).expect("writer should start");

        for payload in packets {
            let packet = PcapPacket::new(
                Duration::ZERO,
                payload
                    .len()
                    .try_into()
                    .expect("payload should fit into u32"),
                payload,
            );
            writer.write_packet(&packet).expect("packet should write");
        }

        writer.flush().expect("pcap should flush");
    }

    fn unique_temp_path(file_name: &str) -> PathBuf {
        let mut path = env::temp_dir();
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos();
        path.push(format!(
            "bit-viewer-{file_name}-{}-{unique}",
            std::process::id()
        ));
        path
    }
}
