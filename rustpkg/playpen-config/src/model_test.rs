use super::*;

/// 使用 deepseek-v4-flash 的实际 preset 价格验证计费
/// input=1.0, output=2.0, cache_read=0.02 (CNY / 1M tokens)
#[test]
fn test_compute_cost_deepseek_v4_flash() {
    let cost = Cost {
        input: 1.0,
        output: 2.0,
        cache_read: 0.02,
        currency: Currency::CNY,
    };

    let usage = Usage {
        input: 55_722_240 + 2_647_127,
        cache_read: 55_722_240,
        output: 173_264,
        total: 58_542_631,
        cache_write: 0,
    };

    let result = cost.compute(&usage);

    assert!((result - 4.1).abs() < 1e-2, "expected 4.1, got {result}");
}

#[test]
fn test_compute_cost_without_cache() {
    let cost = Cost {
        input: 1.0,
        output: 2.0,
        cache_read: 0.0,
        currency: Currency::CNY,
    };
    let usage = Usage {
        input: 1_000_000,
        output: 500_000,
        cache_read: 0,
        cache_write: 0,
        total: 0,
    };
    let result = cost.compute(&usage);
    // (1_000_000 * 1.0 + 500_000 * 2.0) / 1_000_000 = 2.0
    assert!((result - 2.0).abs() < 1e-10);
}

#[test]
fn test_compute_cost_cache_exceeds_input() {
    let cost = Cost {
        input: 1.0,
        output: 2.0,
        cache_read: 0.5,
        currency: Currency::CNY,
    };
    let usage = Usage {
        input: 1_000_000,
        output: 0,
        cache_read: 2_000_000,
        cache_write: 0,
        total: 0,
    };
    let result = cost.compute(&usage);
    // cached = min(2_000_000, 1_000_000) = 1_000_000
    // (0 * 1.0 + 1_000_000 * 0.5 + 0 * 2.0) / 1_000_000 = 0.5
    assert!((result - 0.5).abs() < 1e-10);
}
