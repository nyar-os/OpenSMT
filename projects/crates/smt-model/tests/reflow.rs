use smt_model::{Board, ReflowProfile, SmtDocument, Zone, evaluate_reflow};

#[test]
fn default_profile_peaks_and_crosses_the_liquidus() {
    let document = SmtDocument::new(Board::new(100.0, 80.0, 1.6));
    let report = evaluate_reflow(&document).expect("valid profile");
    assert_eq!(report.zones.len(), 4);
    assert!((report.peak_temperature - 245.0).abs() < 1e-9);
    assert!((report.total_seconds - 140.0).abs() < 1e-9);
    // 180 -> 245 over 30 s contributes 30 * 28 / 65; 245 -> 30 over 40 s
    // contributes 40 * 28 / 215.
    let expected = 30.0 * 28.0 / 65.0 + 40.0 * 28.0 / 215.0;
    assert!((report.time_above_liquidus - expected).abs() < 1e-9);
    assert!((report.zones[2].seconds_above_liquidus - 30.0 * 28.0 / 65.0).abs() < 1e-9);
    assert!((report.zones[3].seconds_above_liquidus - 40.0 * 28.0 / 215.0).abs() < 1e-9);
}

#[test]
fn a_single_zone_is_integrated_exactly() {
    let mut document = SmtDocument::new(Board::new(100.0, 80.0, 1.6));
    document.profile = ReflowProfile { conveyor_speed: 600.0, liquidus: 217.0, zones: vec![Zone::new("reflow", 600.0, 250.0)] };
    let report = evaluate_reflow(&document).expect("valid profile");
    assert!((report.total_seconds - 60.0).abs() < 1e-9);
    assert!((report.time_above_liquidus - 60.0 * 33.0 / 225.0).abs() < 1e-9);
    assert!((report.zones[0].entry_temperature - 25.0).abs() < 1e-9);
}

#[test]
fn a_profile_below_liquidus_has_no_time_above_it() {
    let mut document = SmtDocument::new(Board::new(100.0, 80.0, 1.6));
    document.profile = ReflowProfile {
        conveyor_speed: 600.0,
        liquidus: 217.0,
        zones: vec![Zone::new("warm", 300.0, 150.0), Zone::new("cool", 300.0, 30.0)],
    };
    let report = evaluate_reflow(&document).expect("warning only");
    assert_eq!(report.time_above_liquidus, 0.0);
    assert!((report.peak_temperature - 150.0).abs() < 1e-9);
}

#[test]
fn zone_order_and_heat_soak_are_preserved() {
    let mut document = SmtDocument::new(Board::new(100.0, 80.0, 1.6));
    document.profile = ReflowProfile {
        conveyor_speed: 300.0,
        liquidus: 200.0,
        zones: vec![Zone::new("soak", 300.0, 200.0), Zone::new("peak", 300.0, 260.0)],
    };
    let report = evaluate_reflow(&document).expect("valid");
    assert_eq!(report.zones[0].name, "soak");
    assert!((report.zones[0].seconds - 60.0).abs() < 1e-9);
    // The soak holds exactly at the liquidus, so it contributes no time above.
    assert_eq!(report.zones[0].seconds_above_liquidus, 0.0);
    assert!((report.zones[1].seconds_above_liquidus - 60.0).abs() < 1e-9);
    assert!((report.time_above_liquidus - 60.0).abs() < 1e-9);
}
