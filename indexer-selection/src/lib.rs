use std::{collections::hash_map::DefaultHasher, f64::consts::E, hash::Hasher as _};

pub use candidate_selection::{ArrayVec, Normalized};
pub use performance::*;
pub use weights::*;

mod performance;
mod weights;
#[cfg(test)]
mod test;

#[derive(Debug)]
pub struct Candidate<I, D> {
    /// The unique identifier of the candidate.
    pub id: I,
    /// The data associated with the candidate.
    ///
    /// It can be used to store additional information about the indexer that is not used for
    /// selection.
    pub data: D,

    pub perf: ExpectedPerformance,
    pub fee: Normalized,
    /// seconds behind chain head
    pub seconds_behind: u32,
    pub slashable_grt: u64,
}

pub fn select<I, D, const LIMIT: usize>(
    candidates: &[Candidate<I, D>],
) -> ArrayVec<&Candidate<I, D>, LIMIT>
where
    I: std::hash::Hash,
{
    candidate_selection::select(candidates, &Weights::default())
}

/// Like [`select`], but using caller-provided scoring [`Weights`] instead of the defaults.
pub fn select_with_weights<'c, I, D, const LIMIT: usize>(
    candidates: &'c [Candidate<I, D>],
    weights: &Weights,
) -> ArrayVec<&'c Candidate<I, D>, LIMIT>
where
    I: std::hash::Hash,
{
    candidate_selection::select(candidates, weights)
}

impl<I, D> candidate_selection::Candidate for Candidate<I, D>
where
    I: std::hash::Hash,
{
    type Id = u64;
    type Ctx = Weights;

    fn id(&self) -> Self::Id {
        let mut hasher = DefaultHasher::new();
        self.id.hash(&mut hasher);
        hasher.finish()
    }

    fn fee(&self) -> Normalized {
        self.fee
    }

    fn score(&self, w: &Weights) -> Normalized {
        weighted_product(
            [
                score_success_rate(self.perf.success_rate, &w.success_rate),
                score_latency(self.perf.latency_ms, &w.latency),
                score_seconds_behind(self.seconds_behind, &w.seconds_behind),
                score_slashable_grt(self.slashable_grt, &w.slashable_grt),
            ],
            &w.exponents,
        )
    }

    fn score_many<const LIMIT: usize>(candidates: &[&Self], w: &Weights) -> Normalized {
        let fee = candidates.iter().map(|c| c.fee.as_f64()).sum::<f64>();
        if Normalized::new(fee).is_none() {
            return Normalized::ZERO;
        }

        // candidate latencies
        let ls: ArrayVec<u16, LIMIT> = candidates.iter().map(|c| c.perf.latency_ms).collect();
        // probability of candidate responses returning to client, based on `ls`
        let ps = {
            let mut ps: ArrayVec<Normalized, LIMIT> =
                candidates.iter().map(|c| c.perf.success_rate).collect();
            let mut ls = ls.clone();
            let mut sort = permutation::sort_unstable(&mut ls);
            sort.apply_slice_in_place(&mut ls);
            sort.apply_slice_in_place(&mut ps);
            let pf: ArrayVec<f64, LIMIT> = ps
                .iter()
                .map(|p| 1.0 - p.as_f64())
                .scan(1.0, |s, x| {
                    *s *= x;
                    Some(*s)
                })
                .collect();
            let mut ps: ArrayVec<f64, LIMIT> = std::iter::once(&1.0)
                .chain(&pf)
                .take(LIMIT)
                .zip(&ps)
                .map(|(&p, &s)| p * s.as_f64())
                .collect();
            sort.inverse().apply_slice_in_place(&mut ps);
            ps
        };

        let success_rate = Normalized::new(ps.iter().sum()).unwrap_or(Normalized::ONE);
        let latency = candidates
            .iter()
            .map(|c| c.perf.latency_ms as f64)
            .zip(&ps)
            .map(|(x, p)| x.recip() * p)
            .sum::<f64>()
            .recip() as u16;
        let seconds_behind = candidates.iter().map(|c| c.seconds_behind).max().unwrap();
        let slashable_grt = candidates.iter().map(|c| c.slashable_grt).min().unwrap();

        weighted_product(
            [
                score_success_rate(success_rate, &w.success_rate),
                score_latency(latency, &w.latency),
                score_seconds_behind(seconds_behind, &w.seconds_behind),
                score_slashable_grt(slashable_grt, &w.slashable_grt),
            ],
            &w.exponents,
        )
    }
}

/// Combine per-curve scores into a single [`Normalized`] score, applying each configured exponent
/// as `score_i.powf(exponent_i)`. When an exponent is exactly `1.0` the `powf` is skipped so that
/// the default weights reproduce the previous plain-product behaviour bit-for-bit (and in the same
/// left-fold order as `Iterator::product`). The result is clamped to `[0, 1]` to guard against
/// exponents `< 0` producing values `> 1`.
fn weighted_product(scores: [Normalized; 4], exponents: &[f64; 4]) -> Normalized {
    let product = scores
        .iter()
        .zip(exponents)
        .fold(1.0_f64, |acc, (s, &e)| acc * apply_exponent(s.as_f64(), e));
    Normalized::clamp(product, 0.0, 1.0).unwrap()
}

#[inline]
fn apply_exponent(score: f64, exponent: f64) -> f64 {
    if exponent == 1.0 {
        score
    } else {
        score.powf(exponent)
    }
}

// When picking curves to use consider the following reference:
// https://en.wikipedia.org/wiki/Logistic_function

/// https://www.desmos.com/calculator/jdogbfxw2j
fn score_seconds_behind(seconds_behind: u32, w: &SecondsBehindWeights) -> Normalized {
    let u = w.offset
        + (w.max / (1.0 + E.powf(w.steepness * (seconds_behind as i64 - w.midpoint_s) as f64)));
    Normalized::new(u).unwrap()
}

/// https://www.desmos.com/calculator/iqhjcdnphv
fn score_slashable_grt(slashable_grt: u64, w: &SlashableGrtWeights) -> Normalized {
    let x = slashable_grt as f64;
    // Currently setting a minimum score of ~0.8 at the minimum stake requirement of 100,000 GRT.
    Normalized::new(1.0 - E.powf(-w.rate * x)).unwrap()
}

/// https://www.desmos.com/calculator/v2vrfktlpl
pub fn score_latency(latency_ms: u16, w: &LatencyWeights) -> Normalized {
    let s = |x: u16| 1.0 + E.powf(((x as f64) - w.midpoint_ms) / w.scale_ms);
    // Since high latency becomes bad success rate via timeouts, latency scores should have a floor.
    Normalized::clamp(s(0) / s(latency_ms), w.floor, 1.0).unwrap()
}

/// https://www.desmos.com/calculator/df2keku3ad
fn score_success_rate(success_rate: Normalized, w: &SuccessRateWeights) -> Normalized {
    Normalized::clamp(success_rate.as_f64().powi(w.exponent), w.floor, 1.0).unwrap()
}
