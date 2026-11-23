use smt_model::{Board, ReflowProfile, ReflowWindowLimits, SmtDocument, Zone, evaluate_reflow_window};

fn document() -> SmtDocument {
    SmtDocument::new(Board::new(100.0, 80.0, 1.6))
}

#[test]
fn the_default_profile_reports_metrics_and_zone_rates() {
    let document = document();
    let report = evaluate_reflow_window(&document, &ReflowWindowLimits::default()).expect("valid profile");

    assert_eq!(report.format, "open-smt-reflow-window");
    assert_eq!(report.version, 1);
    assert_eq!(report.units, "mm");
    assert_eq!(report.temperature_unit, "celsius");
    assert_eq!(report.conveyor_speed, 600.0);
    assert_eq!(report.liquidus, 217.0);
    assert_eq!(report.peak_temperature, 245.0);
    assert!((report.total_seconds - 140.0).abs() < 1e-9);
    let expected = 30.0 * 28.0 / 65.0 + 40.0 * 28.0 / 215.0;
    assert!((report.time_above_liquidus - expected).abs() < 1e-9);

    assert_eq!(report.zones.len(), 4);
    assert!((report.zones[0].seconds - 30.0).abs() < 1e-9);
    assert!((report.zones[0].ramp_rate - 125.0 / 30.0).abs() < 1e-9);
    assert!((report.zones[1].ramp_rate - 0.75).abs() < 1e-9);
    assert!((report.zones[2].ramp_rate - 65.0 / 30.0).abs() < 1e-9);
    assert!((report.zones[3].ramp_rate + 215.0 / 40.0).abs() < 1e-9);

    assert!((report.max_heating_rate - 125.0 / 30.0).abs() < 1e-9);
    assert!((report.max_cooling_rate - 5.375).abs() < 1e-9);
    assert!(report.checks.is_empty());
    assert!(report.ok);
}

#[test]
fn supplied_limits_produce_checks_with_margins() {
    let document = document();
    let limits = ReflowWindowLimits {
        peak_min: Some(235.0),
        peak_max: Some(250.0),
        time_above_liquidus_min: Some(15.0),
        time_above_liquidus_max: Some(60.0),
        max_heating_rate: Some(4.0),
        max_cooling_rate: Some(6.0),
    };
    let report = evaluate_reflow_window(&document, &limits).expect("valid limits");

    assert_eq!(report.checks.len(), 6);
    let metrics: Vec<(&str, &str)> = report.checks.iter().map(|check| (check.metric.as_str(), check.kind.as_str())).collect();
    assert_eq!(
        metrics,
        [
            ("peakTemperature", "min"),
            ("peakTemperature", "max"),
            ("timeAboveLiquidus", "min"),
            ("timeAboveLiquidus", "max"),
            ("maxHeatingRate", "max"),
            ("maxCoolingRate", "max"),
        ]
    );

    assert!((report.checks[0].margin - 10.0).abs() < 1e-9);
    assert!(report.checks[0].pass);
    assert!((report.checks[1].margin - 5.0).abs() < 1e-9);
    assert!(report.checks[1].pass);
    let tal = report.time_above_liquidus;
    assert!((report.checks[2].margin - (tal - 15.0)).abs() < 1e-9);
    assert!(report.checks[2].pass);
    assert!((report.checks[3].margin - (60.0 - tal)).abs() < 1e-9);
    assert!(report.checks[3].pass);
    // 125 / 30 ≈ 4.1667 °C/s exceeds the 4 °C/s limit.
    assert!(report.checks[4].margin < 0.0);
    assert!(!report.checks[4].pass);
    assert!((report.checks[5].margin - 0.625).abs() < 1e-9);
    assert!(report.checks[5].pass);
    assert!(!report.ok);
}

#[test]
fn a_profile_inside_every_limit_is_ok() {
    let document = document();
    let limits = ReflowWindowLimits {
        peak_min: Some(230.0),
        peak_max: Some(260.0),
        time_above_liquidus_min: Some(10.0),
        time_above_liquidus_max: Some(90.0),
        max_heating_rate: Some(5.0),
        max_cooling_rate: Some(6.0),
    };
    let report = evaluate_reflow_window(&document, &limits).expect("valid limits");

    assert!(report.checks.iter().all(|check| check.pass));
    assert!(report.ok);
}

#[test]
fn a_heating_only_profile_has_no_cooling_rate() {
    let mut document = document();
    document.profile = ReflowProfile { conveyor_speed: 600.0, liquidus: 217.0, zones: vec![Zone::new("ramp", 600.0, 250.0)] };
    let report = evaluate_reflow_window(&document, &ReflowWindowLimits::default()).expect("valid profile");

    assert!(report.max_heating_rate > 0.0);
    assert_eq!(report.max_cooling_rate, 0.0);
    assert!(report.zones.iter().all(|zone| zone.ramp_rate >= 0.0));
}

#[test]
fn invalid_limits_report_reflow_invalid() {
    let document = document();
    let invalid = [
        ReflowWindowLimits { peak_min: Some(-1.0), ..ReflowWindowLimits::default() },
        ReflowWindowLimits { peak_max: Some(f64::NAN), ..ReflowWindowLimits::default() },
        ReflowWindowLimits { time_above_liquidus_min: Some(f64::INFINITY), ..ReflowWindowLimits::default() },
        ReflowWindowLimits { max_heating_rate: Some(-0.5), ..ReflowWindowLimits::default() },
        ReflowWindowLimits { max_cooling_rate: Some(f64::NAN), ..ReflowWindowLimits::default() },
        ReflowWindowLimits { peak_min: Some(300.0), peak_max: Some(200.0), ..ReflowWindowLimits::default() },
        ReflowWindowLimits {
            time_above_liquidus_min: Some(10.0),
            time_above_liquidus_max: Some(5.0),
            ..ReflowWindowLimits::default()
        },
    ];
    for limits in invalid {
        let error = evaluate_reflow_window(&document, &limits).expect_err("invalid limits");
        assert_eq!(error.code, "reflow/invalid");
    }
}

#[test]
fn the_report_is_deterministic_and_leaves_the_document_unchanged() {
    let mut document = document();
    document.revision = 7;
    let before = document.clone();
    let limits = ReflowWindowLimits { peak_max: Some(250.0), ..ReflowWindowLimits::default() };
    let first = evaluate_reflow_window(&document, &limits).expect("valid");
    let second = evaluate_reflow_window(&document, &limits).expect("valid");

    assert_eq!(serde_json::to_string(&first).expect("json"), serde_json::to_string(&second).expect("json"));
    assert_eq!(document, before);
}
