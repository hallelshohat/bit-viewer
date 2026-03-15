use crate::filters::DerivedView;

#[derive(Clone, Debug)]
pub struct RunHistogramSample {
    pub run_length_bits: usize,
    pub count: usize,
    pub fraction: f32,
}

#[derive(Clone, Debug, Default)]
pub struct RunHistogramResult {
    pub total_runs: usize,
    pub total_bits: usize,
    pub samples: Vec<RunHistogramSample>,
    pub dominant_run_length_bits: Option<usize>,
    pub dominant_fraction: Option<f32>,
}

impl RunHistogramResult {
    pub fn max_run_length_bits(&self) -> usize {
        self.samples
            .last()
            .map(|sample| sample.run_length_bits)
            .unwrap_or_default()
    }

    pub fn sample_for_run_length(&self, run_length_bits: usize) -> Option<&RunHistogramSample> {
        self.samples
            .get(run_length_bits.saturating_sub(1))
            .filter(|sample| sample.run_length_bits == run_length_bits)
    }
}

pub fn analyze_run_histogram_with_progress(
    view: &DerivedView,
    mut on_progress: impl FnMut(usize, usize) -> bool,
) -> Option<RunHistogramResult> {
    const PROGRESS_STEP_BITS: usize = 65_536;

    let total_bits = view.total_bits();
    if total_bits == 0 {
        on_progress(0, 0);
        return Some(RunHistogramResult::default());
    }

    let mut processed_bits = 0usize;
    let mut total_runs = 0usize;
    let mut counts_by_run_length = Vec::<usize>::new();

    for group in view.groups() {
        let bit_len = group.len_bits();
        if bit_len == 0 {
            continue;
        }

        let bytes = group.packed_bytes();
        let mut current_bit = bit_at(bytes, 0);
        let mut current_run_length = 1usize;
        let mut next_progress_update = PROGRESS_STEP_BITS.min(bit_len);

        for bit_index in 1..bit_len {
            if bit_index >= next_progress_update {
                let completed = processed_bits + bit_index;
                if !on_progress(completed, total_bits) {
                    return None;
                }
                next_progress_update = (next_progress_update + PROGRESS_STEP_BITS).min(bit_len);
            }

            let bit = bit_at(bytes, bit_index);
            if bit == current_bit {
                current_run_length += 1;
                continue;
            }

            record_run(&mut counts_by_run_length, current_run_length);
            total_runs += 1;
            current_bit = bit;
            current_run_length = 1;
        }

        record_run(&mut counts_by_run_length, current_run_length);
        total_runs += 1;
        processed_bits += bit_len;

        if !on_progress(processed_bits, total_bits) {
            return None;
        }
    }

    let mut dominant_run_length_bits = None;
    let mut dominant_count = 0usize;
    let samples = counts_by_run_length
        .into_iter()
        .enumerate()
        .map(|(index, count)| {
            let run_length_bits = index + 1;
            let fraction = count as f32 / total_runs as f32;
            let replace_dominant = count > dominant_count
                || (count == dominant_count
                    && match dominant_run_length_bits {
                        Some(best) => run_length_bits < best,
                        None => true,
                    });
            if replace_dominant {
                dominant_run_length_bits = Some(run_length_bits);
                dominant_count = count;
            }

            RunHistogramSample {
                run_length_bits,
                count,
                fraction,
            }
        })
        .collect::<Vec<_>>();

    Some(RunHistogramResult {
        total_runs,
        total_bits,
        dominant_run_length_bits,
        dominant_fraction: dominant_run_length_bits
            .and_then(|run_length_bits| samples.get(run_length_bits.saturating_sub(1)))
            .map(|sample| sample.fraction),
        samples,
    })
}

fn record_run(counts_by_run_length: &mut Vec<usize>, run_length: usize) {
    if counts_by_run_length.len() < run_length {
        counts_by_run_length.resize(run_length, 0);
    }
    counts_by_run_length[run_length - 1] += 1;
}

fn bit_at(bytes: &[u8], bit_index: usize) -> u8 {
    let byte = bytes[bit_index / 8];
    (byte >> (7 - (bit_index % 8))) & 1
}

#[cfg(test)]
mod tests {
    use crate::filters::{FilterPipeline, build_derived_view, build_derived_view_from_groups};

    use super::analyze_run_histogram_with_progress;

    #[test]
    fn normalizes_alternating_runs_to_length_one() {
        let view =
            build_derived_view(&[0b1010_1010], &FilterPipeline::default()).expect("view builds");

        let result = analyze_run_histogram_with_progress(&view, |_, _| true).expect("runs build");

        assert_eq!(result.total_runs, 8);
        assert_eq!(result.max_run_length_bits(), 1);
        assert_eq!(
            result.sample_for_run_length(1).map(|sample| sample.count),
            Some(8)
        );
        assert_eq!(
            result
                .sample_for_run_length(1)
                .map(|sample| sample.fraction),
            Some(1.0)
        );
    }

    #[test]
    fn treats_group_boundaries_as_run_boundaries() {
        let view =
            build_derived_view_from_groups(&[vec![0xFF], vec![0xFF]], &FilterPipeline::default())
                .expect("view builds");

        let result = analyze_run_histogram_with_progress(&view, |_, _| true).expect("runs build");

        assert_eq!(result.total_runs, 2);
        assert_eq!(result.max_run_length_bits(), 8);
        assert_eq!(
            result.sample_for_run_length(8).map(|sample| sample.count),
            Some(2)
        );
    }

    #[test]
    fn can_cancel_before_completion() {
        let view = build_derived_view(&[0x00; 16_384], &FilterPipeline::default()).expect("view");
        let mut first_progress = true;

        let result = analyze_run_histogram_with_progress(&view, |completed, total| {
            if first_progress && completed < total {
                first_progress = false;
                return false;
            }
            true
        });

        assert!(result.is_none());
    }
}
