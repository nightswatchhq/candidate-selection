//! Runtime-configurable weights for the indexer scoring curves.
//!
//! Every field carries a `#[serde(default)]` so that a partial configuration yields all-other
//! defaults. The [`Default`] impls reproduce the previously hard-coded scoring constants exactly,
//! so `Weights::default()` is behaviour-identical to the pre-config implementation.

#[derive(Clone, Copy, Debug, PartialEq, serde::Deserialize)]
#[serde(default)]
pub struct Weights {
    pub success_rate: SuccessRateWeights,
    pub latency: LatencyWeights,
    pub seconds_behind: SecondsBehindWeights,
    pub slashable_grt: SlashableGrtWeights,
    /// True weighted-product exponents applied as score_i.powf(w_i).
    /// Order: [success_rate, latency, seconds_behind, slashable_grt]. All 1.0 = current plain product.
    pub exponents: [f64; 4],
}

impl Default for Weights {
    fn default() -> Self {
        Self {
            success_rate: SuccessRateWeights::default(),
            latency: LatencyWeights::default(),
            seconds_behind: SecondsBehindWeights::default(),
            slashable_grt: SlashableGrtWeights::default(),
            exponents: [1.0, 1.0, 1.0, 1.0],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Deserialize)]
#[serde(default)]
pub struct SuccessRateWeights {
    pub exponent: i32,
    pub floor: f64,
}

impl Default for SuccessRateWeights {
    fn default() -> Self {
        Self {
            exponent: 7,
            floor: 1e-8,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Deserialize)]
#[serde(default)]
pub struct LatencyWeights {
    pub midpoint_ms: f64,
    pub scale_ms: f64,
    pub floor: f64,
}

impl Default for LatencyWeights {
    fn default() -> Self {
        Self {
            midpoint_ms: 400.0,
            scale_ms: 300.0,
            floor: 0.001,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Deserialize)]
#[serde(default)]
pub struct SecondsBehindWeights {
    pub offset: f64,
    pub max: f64,
    pub steepness: f64,
    pub midpoint_s: i64,
}

impl Default for SecondsBehindWeights {
    fn default() -> Self {
        Self {
            offset: 1e-16,
            max: 1.532,
            steepness: 0.021,
            midpoint_s: 30,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Deserialize)]
#[serde(default)]
pub struct SlashableGrtWeights {
    pub rate: f64,
}

impl Default for SlashableGrtWeights {
    fn default() -> Self {
        Self { rate: 1.6e-5 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_object_yields_all_defaults() {
        let w: Weights = serde_json::from_str("{}").unwrap();
        assert_eq!(w, Weights::default());
    }

    #[test]
    fn partial_config_fills_other_defaults() {
        // Only override latency.floor; everything else must remain default.
        let w: Weights = serde_json::from_str(r#"{ "latency": { "floor": 0.5 } }"#).unwrap();
        assert_eq!(w.latency.floor, 0.5);
        assert_eq!(w.latency.midpoint_ms, LatencyWeights::default().midpoint_ms);
        assert_eq!(w.latency.scale_ms, LatencyWeights::default().scale_ms);
        assert_eq!(w.success_rate, SuccessRateWeights::default());
        assert_eq!(w.seconds_behind, SecondsBehindWeights::default());
        assert_eq!(w.slashable_grt, SlashableGrtWeights::default());
        assert_eq!(w.exponents, [1.0, 1.0, 1.0, 1.0]);
    }

    #[test]
    fn absent_option_is_none() {
        // Mirrors the gateway config: an `Option<Weights>` field with `#[serde(default)]`.
        #[derive(serde::Deserialize)]
        struct Cfg {
            #[serde(default)]
            selection: Option<Weights>,
        }
        let cfg: Cfg = serde_json::from_str("{}").unwrap();
        assert!(cfg.selection.is_none());
    }
}
