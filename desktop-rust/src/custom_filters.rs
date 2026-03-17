use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::filters::FilterPipeline;

const CUSTOM_FILTERS_DIR: &str = "bit-viewer-desktop";
const CUSTOM_FILTERS_FILE: &str = "custom-filters.json";
const CUSTOM_FILTERS_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomFilterPreset {
    pub name: String,
    pub pipeline: FilterPipeline,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
struct PersistedCustomFilters {
    #[serde(default = "custom_filters_version")]
    version: u32,
    #[serde(default)]
    presets: Vec<CustomFilterPreset>,
}

const fn custom_filters_version() -> u32 {
    CUSTOM_FILTERS_VERSION
}

pub fn load_custom_filters() -> Result<Vec<CustomFilterPreset>, String> {
    let path = custom_filters_path()?;
    load_custom_filters_from_path(&path)
}

pub fn save_custom_filters(filters: &[CustomFilterPreset]) -> Result<(), String> {
    let path = custom_filters_path()?;
    save_custom_filters_to_path(&path, filters)
}

fn load_custom_filters_from_path(path: &Path) -> Result<Vec<CustomFilterPreset>, String> {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(format!(
                "Failed to read custom filters from {}: {error}",
                path.display()
            ));
        }
    };

    let stored = serde_json::from_str::<PersistedCustomFilters>(&contents).map_err(|error| {
        format!(
            "Failed to parse custom filters from {}: {error}",
            path.display()
        )
    })?;

    Ok(stored.presets)
}

fn save_custom_filters_to_path(path: &Path, filters: &[CustomFilterPreset]) -> Result<(), String> {
    let Some(parent) = path.parent() else {
        return Err(format!(
            "Custom filter path {} has no parent directory.",
            path.display()
        ));
    };

    fs::create_dir_all(parent).map_err(|error| {
        format!(
            "Failed to create custom filter directory {}: {error}",
            parent.display()
        )
    })?;

    let stored = PersistedCustomFilters {
        version: CUSTOM_FILTERS_VERSION,
        presets: filters.to_vec(),
    };
    let contents = serde_json::to_string_pretty(&stored)
        .map_err(|error| format!("Failed to serialize custom filters: {error}"))?;

    fs::write(path, contents).map_err(|error| {
        format!(
            "Failed to write custom filters to {}: {error}",
            path.display()
        )
    })
}

pub fn custom_filters_path() -> Result<PathBuf, String> {
    Ok(config_root_dir()?
        .join(CUSTOM_FILTERS_DIR)
        .join(CUSTOM_FILTERS_FILE))
}

fn config_root_dir() -> Result<PathBuf, String> {
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .ok_or_else(|| "APPDATA is not set.".to_owned())
    }

    #[cfg(target_os = "macos")]
    {
        home_dir()
            .map(|path| path.join("Library").join("Application Support"))
            .ok_or_else(|| "HOME is not set.".to_owned())
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        if let Some(path) = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from) {
            return Ok(path);
        }

        home_dir()
            .map(|path| path.join(".config"))
            .ok_or_else(|| "HOME is not set.".to_owned())
    }
}

#[cfg(not(target_os = "windows"))]
fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::{CustomFilterPreset, load_custom_filters_from_path, save_custom_filters_to_path};
    use crate::filters::{FilterPipeline, FilterStep, GroupChunkRange};
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn custom_filters_round_trip_through_json_file() {
        let path = unique_temp_path("round-trip");
        let filters = vec![
            CustomFilterPreset {
                name: "Packet header".to_owned(),
                pipeline: FilterPipeline {
                    steps: vec![
                        FilterStep::Split {
                            group_size_bits: 256,
                        },
                        FilterStep::SelectSubgroupRangesFromGroup {
                            chunk_count: 32,
                            subgroup_size_bits: 8,
                            subgroup_ranges: vec![
                                GroupChunkRange {
                                    start_chunk: 0,
                                    end_chunk: 7,
                                },
                                GroupChunkRange {
                                    start_chunk: 16,
                                    end_chunk: 23,
                                },
                            ],
                        },
                    ],
                },
            },
            CustomFilterPreset {
                name: "Descramble".to_owned(),
                pipeline: FilterPipeline {
                    steps: vec![FilterStep::LfsrDescramble {
                        seed: "0x7f".to_owned(),
                        polynomial: "x^7+x^3+1".to_owned(),
                    }],
                },
            },
        ];

        save_custom_filters_to_path(&path, &filters).expect("custom filters should save");
        let loaded = load_custom_filters_from_path(&path).expect("custom filters should load");

        assert_eq!(loaded, filters);

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(path.parent().expect("path should have parent"));
    }

    #[test]
    fn missing_custom_filter_file_returns_empty_list() {
        let path = unique_temp_path("missing");
        let loaded = load_custom_filters_from_path(&path).expect("missing file should be empty");

        assert!(loaded.is_empty());
    }

    fn unique_temp_path(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos();

        std::env::temp_dir()
            .join(format!("bit-viewer-custom-filter-tests-{label}-{nanos}"))
            .join("custom-filters.json")
    }
}
