use crate::filters::DerivedView;

#[derive(Clone, Debug)]
pub struct AutoCorrelationSample {
    pub width_bits: usize,
    pub score: f32,
    pub comparisons: usize,
}

#[derive(Clone, Debug, Default)]
pub struct AutoCorrelationResult {
    pub requested_max_width_bits: usize,
    pub samples: Vec<AutoCorrelationSample>,
    pub best_width_bits: Option<usize>,
    pub best_score: Option<f32>,
}

impl AutoCorrelationResult {
    pub fn available_max_width_bits(&self) -> usize {
        self.samples
            .last()
            .map(|sample| sample.width_bits)
            .unwrap_or_default()
    }

    pub fn sample_for_width(&self, width_bits: usize) -> Option<&AutoCorrelationSample> {
        self.samples
            .get(width_bits.saturating_sub(1))
            .filter(|sample| sample.width_bits == width_bits)
    }
}

#[allow(dead_code)]
pub fn autocorrelation_width_limit(view: &DerivedView, max_width_bits: usize) -> usize {
    autocorrelation_width_limit_limited(view, max_width_bits, usize::MAX)
}

pub fn autocorrelation_width_limit_limited(
    view: &DerivedView,
    max_width_bits: usize,
    sample_bytes_limit: usize,
) -> usize {
    let requested_max_width_bits = max_width_bits.max(1);
    sampled_groups(view, sample_bytes_limit)
        .into_iter()
        .filter_map(|group| group.bit_len.checked_sub(1))
        .max()
        .unwrap_or_default()
        .min(requested_max_width_bits)
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn analyze_width_autocorrelation(
    view: &DerivedView,
    max_width_bits: usize,
) -> AutoCorrelationResult {
    analyze_width_autocorrelation_limited(view, max_width_bits, usize::MAX)
}

pub fn analyze_width_autocorrelation_limited(
    view: &DerivedView,
    max_width_bits: usize,
    sample_bytes_limit: usize,
) -> AutoCorrelationResult {
    analyze_width_autocorrelation_limited_with_progress(
        view,
        max_width_bits,
        sample_bytes_limit,
        |_, _| {},
    )
}

pub fn analyze_width_autocorrelation_limited_with_progress(
    view: &DerivedView,
    max_width_bits: usize,
    sample_bytes_limit: usize,
    mut on_progress: impl FnMut(usize, usize),
) -> AutoCorrelationResult {
    let requested_max_width_bits = max_width_bits.max(1);
    let sampled_groups = sampled_groups(view, sample_bytes_limit);
    let available_max_width_bits = sampled_groups
        .iter()
        .filter_map(|group| group.bit_len.checked_sub(1))
        .max()
        .unwrap_or_default()
        .min(requested_max_width_bits);
    let mut samples = Vec::new();
    let mut best_width_bits = None;
    let mut best_score = None;

    for width_bits in 1..=available_max_width_bits {
        let mut comparisons = 0usize;
        let mut correlation_sum = 0i64;

        for group in &sampled_groups {
            let bit_len = group.bit_len;
            if bit_len <= width_bits {
                continue;
            }

            let width_comparisons = bit_len.saturating_sub(width_bits);
            comparisons += width_comparisons;

            for bit_index in 0..width_comparisons {
                if bit_at(group.bytes, bit_index) == bit_at(group.bytes, bit_index + width_bits) {
                    correlation_sum += 1;
                } else {
                    correlation_sum -= 1;
                }
            }
        }

        let score = correlation_sum as f32 / comparisons as f32;
        samples.push(AutoCorrelationSample {
            width_bits,
            score,
            comparisons,
        });

        let replace_best = match best_score {
            None => true,
            Some(previous_best) => {
                score > previous_best
                    || ((score - previous_best).abs() <= f32::EPSILON
                        && width_bits < best_width_bits.unwrap_or(usize::MAX))
            }
        };

        if replace_best {
            best_width_bits = Some(width_bits);
            best_score = Some(score);
        }

        on_progress(width_bits, available_max_width_bits);
    }

    AutoCorrelationResult {
        requested_max_width_bits,
        samples,
        best_width_bits,
        best_score,
    }
}

fn bit_at(bytes: &[u8], bit_index: usize) -> u8 {
    let byte = bytes[bit_index / 8];
    (byte >> (7 - (bit_index % 8))) & 1
}

struct SampledGroup<'a> {
    bytes: &'a [u8],
    bit_len: usize,
}

fn sampled_groups(view: &DerivedView, sample_bytes_limit: usize) -> Vec<SampledGroup<'_>> {
    let mut remaining_bits = sample_bytes_limit.max(1).saturating_mul(8);
    let mut groups = Vec::new();

    for group in view.groups() {
        if remaining_bits == 0 {
            break;
        }

        let bit_len = group.len_bits().min(remaining_bits);
        if bit_len == 0 {
            continue;
        }

        groups.push(SampledGroup {
            bytes: group.packed_bytes(),
            bit_len,
        });
        remaining_bits = remaining_bits.saturating_sub(bit_len);
    }

    groups
}

#[cfg(test)]
mod tests {
    use crate::filters::{FilterPipeline, build_derived_view, build_derived_view_from_groups};

    use super::analyze_width_autocorrelation;

    #[test]
    fn picks_smallest_repeating_width_when_multiples_tie() {
        let view = build_derived_view(&[0xA5, 0xA5, 0xA5, 0xA5], &FilterPipeline::default())
            .expect("view should build");

        let result = analyze_width_autocorrelation(&view, 24);

        assert_eq!(result.best_width_bits, Some(8));
        assert_eq!(
            result.sample_for_width(8).map(|sample| sample.score),
            Some(1.0)
        );
        assert_eq!(
            result.sample_for_width(16).map(|sample| sample.score),
            Some(1.0)
        );
    }

    #[test]
    fn ignores_group_boundaries_when_scoring_widths() {
        let view =
            build_derived_view_from_groups(&[vec![0xFF], vec![0x00]], &FilterPipeline::default())
                .expect("view should build");

        let result = analyze_width_autocorrelation(&view, 4);

        assert_eq!(result.best_width_bits, Some(1));
        assert_eq!(
            result.sample_for_width(1).map(|sample| sample.score),
            Some(1.0)
        );
        assert_eq!(
            result.sample_for_width(1).map(|sample| sample.comparisons),
            Some(14)
        );
    }

    #[test]
    fn stops_at_available_widths() {
        let view =
            build_derived_view(&[0xF0], &FilterPipeline::default()).expect("view should build");

        let result = analyze_width_autocorrelation(&view, 32);

        assert_eq!(result.requested_max_width_bits, 32);
        assert_eq!(result.available_max_width_bits(), 7);
        assert_eq!(result.samples.len(), 7);
    }
}
