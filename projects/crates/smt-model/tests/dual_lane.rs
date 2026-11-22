use smt_model::{
    Board, Component, DualLaneTask, Feeder, Side, SmtDocument, allocate_feeders, plan_dual_lane, plan_dual_lane_tasks,
    sequence_placement,
};

fn tasks(values: &[(&str, f64)]) -> Vec<DualLaneTask> {
    values.iter().map(|(id, seconds)| DualLaneTask { id: (*id).to_string(), seconds: *seconds }).collect()
}

fn document() -> SmtDocument {
    let mut document = SmtDocument::new(Board::new(100.0, 80.0, 1.6));
    document.components.push(Component::new("R1", "0603", 10.0, 10.0));
    document.components.push(Component::new("R2", "0603", 50.0, 10.0));
    document.components.push(Component::new("C1", "0603", 50.0, 50.0));
    let mut u1 = Component::new("U1", "QFN-32", 20.0, 20.0);
    u1.side = Side::Bottom;
    document.components.push(u1);
    document.feeders.push(Feeder::new("F1", "0603", 4.0, 100));
    document.feeders.push(Feeder::new("F2", "QFN-32", 12.0, 20));
    document
}

#[test]
fn greedy_assignment_balances_the_known_task_set() {
    let report = plan_dual_lane_tasks(&tasks(&[("A", 10.0), ("B", 20.0), ("C", 5.0), ("D", 15.0)]));

    assert_eq!(report.format, "open-smt-dual-lane");
    assert_eq!(report.version, 1);
    assert_eq!(report.lanes.len(), 2);

    assert_eq!(report.lanes[0].index, 0);
    assert_eq!(report.lanes[0].tasks, vec!["A", "C", "D"]);
    assert!((report.lanes[0].seconds - 30.0).abs() < 1e-9);

    assert_eq!(report.lanes[1].index, 1);
    assert_eq!(report.lanes[1].tasks, vec!["B"]);
    assert!((report.lanes[1].seconds - 20.0).abs() < 1e-9);

    assert!((report.single_lane_seconds - 50.0).abs() < 1e-9);
    assert!((report.makespan - 30.0).abs() < 1e-9);
    assert!((report.balance_ratio - 0.6).abs() < 1e-9);
}

#[test]
fn every_task_can_land_on_one_lane() {
    // Zero-duration tasks tie on lane 0, so the positive task stays there too
    // and the second lane stays empty.
    let report = plan_dual_lane_tasks(&tasks(&[("A", 0.0), ("B", 0.0), ("C", 4.0)]));

    assert_eq!(report.lanes[0].tasks, vec!["A", "B", "C"]);
    assert!((report.lanes[0].seconds - 4.0).abs() < 1e-9);
    assert!(report.lanes[1].tasks.is_empty());
    assert_eq!(report.lanes[1].seconds, 0.0);
    assert!((report.makespan - 4.0).abs() < 1e-9);
    assert!((report.single_lane_seconds - 4.0).abs() < 1e-9);
    assert!((report.balance_ratio - 1.0).abs() < 1e-9);
}

#[test]
fn an_empty_task_list_keeps_both_lanes_empty() {
    let report = plan_dual_lane_tasks(&[]);

    assert_eq!(report.lanes.len(), 2);
    for lane in &report.lanes {
        assert!(lane.tasks.is_empty());
        assert_eq!(lane.seconds, 0.0);
    }
    assert_eq!(report.single_lane_seconds, 0.0);
    assert_eq!(report.makespan, 0.0);
    assert_eq!(report.balance_ratio, 0.0);
}

#[test]
fn lane_work_and_the_balance_ratio_are_exact() {
    let report = plan_dual_lane_tasks(&tasks(&[("A", 2.0), ("B", 2.0)]));

    assert_eq!(report.lanes[0].tasks, vec!["A"]);
    assert_eq!(report.lanes[1].tasks, vec!["B"]);
    assert!((report.lanes[0].seconds - 2.0).abs() < 1e-9);
    assert!((report.lanes[1].seconds - 2.0).abs() < 1e-9);
    assert!((report.single_lane_seconds - 4.0).abs() < 1e-9);
    assert!((report.makespan - 2.0).abs() < 1e-9);
    assert!((report.balance_ratio - 0.5).abs() < 1e-9);

    let lane_total: f64 = report.lanes.iter().map(|lane| lane.seconds).sum();
    assert!((lane_total - report.single_lane_seconds).abs() < 1e-9);
    let expected_makespan = report.lanes.iter().map(|lane| lane.seconds).fold(0.0, f64::max);
    assert!((report.makespan - expected_makespan).abs() < 1e-9);
}

#[test]
fn each_lane_keeps_the_input_order() {
    let report = plan_dual_lane_tasks(&tasks(&[("A", 5.0), ("B", 4.0), ("C", 3.0), ("D", 2.0)]));

    assert_eq!(report.lanes[0].tasks, vec!["A", "D"]);
    assert_eq!(report.lanes[1].tasks, vec!["B", "C"]);

    let order = ["A", "B", "C", "D"];
    for lane in &report.lanes {
        let mut last = None;
        for id in &lane.tasks {
            let position = order.iter().position(|candidate| candidate == id).expect("known task");
            if let Some(last) = last {
                assert!(position > last, "lane {} reordered {id}", lane.index);
            }
            last = Some(position);
        }
    }
}

#[test]
fn a_document_uses_the_placement_program_tasks() {
    let document = document();
    let feeders = allocate_feeders(&document).expect("valid");
    let placement = sequence_placement(&document, &feeders).expect("sequence");
    let report = plan_dual_lane(&document).expect("dual lane");

    let mut scheduled: Vec<&str> = report.lanes.iter().flat_map(|lane| lane.tasks.iter().map(String::as_str)).collect();
    let mut expected: Vec<&str> = placement.entries.iter().map(|entry| entry.refdes.as_str()).collect();
    scheduled.sort_unstable();
    expected.sort_unstable();
    assert_eq!(scheduled, expected);

    // Every lane is a subsequence of the placement program.
    for lane in &report.lanes {
        let mut cursor = 0;
        for id in &lane.tasks {
            let offset = placement.entries[cursor..]
                .iter()
                .position(|entry| entry.refdes == *id)
                .unwrap_or_else(|| panic!("lane {} lost the program order at {id}", lane.index));
            cursor += offset + 1;
        }
    }

    assert!((report.single_lane_seconds - placement.placement_seconds).abs() < 1e-9);
    let lane_total: f64 = report.lanes.iter().map(|lane| lane.seconds).sum();
    assert!((lane_total - report.single_lane_seconds).abs() < 1e-9);
    assert!((report.makespan - report.lanes.iter().map(|lane| lane.seconds).fold(0.0, f64::max)).abs() < 1e-12);
    assert!((report.balance_ratio - report.makespan / report.single_lane_seconds).abs() < 1e-12);
}

#[test]
fn dual_lane_scheduling_is_deterministic_and_read_only() {
    let document = document();
    let before = serde_json::to_string(&document).expect("json");
    let first = plan_dual_lane(&document).expect("dual lane");
    let second = plan_dual_lane(&document).expect("dual lane");
    assert_eq!(serde_json::to_string(&first).expect("json"), serde_json::to_string(&second).expect("json"));
    assert_eq!(before, serde_json::to_string(&document).expect("json"));
}
