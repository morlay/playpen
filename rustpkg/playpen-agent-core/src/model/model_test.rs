use super::*;

#[test]
fn cost_compute_normal() {
    let cost = Cost { input: 0.01, output: 0.02, cache_read: 0.0 };
    let usage = Usage { input: 100, output: 200, cache_read: 0, cache_write: 0, total: 300 };
    assert!((cost.compute(&usage) - 5.0).abs() < 0.001);
}

#[test]
fn cost_compute_with_cache() {
    let cost = Cost { input: 0.01, output: 0.02, cache_read: 0.005 };
    let usage = Usage { input: 100, output: 200, cache_read: 60, cache_write: 0, total: 300 };
    // (100-60)*0.01 + 60*0.005 + 200*0.02 = 0.4 + 0.3 + 4.0 = 4.7
    assert!((cost.compute(&usage) - 4.7).abs() < 0.001);
}

#[test]
fn cost_compute_zero() {
    let cost = Cost { input: 0.01, output: 0.02, cache_read: 0.0 };
    let usage = Usage::default();
    assert_eq!(cost.compute(&usage), 0.0);
}

#[test]
fn usage_serde_roundtrip() {
    let usage = Usage { input: 100, output: 200, cache_read: 30, cache_write: 10, total: 300 };
    let json = serde_json::to_string(&usage).unwrap();
    let back: Usage = serde_json::from_str(&json).unwrap();
    assert_eq!(back.input, 100);
    assert_eq!(back.total, 300);
}
