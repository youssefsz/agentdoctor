use std::path::{Path, PathBuf};

use agentdoctor_engine::{AgentProfile, ScanInput, ScanOptions, scan_workspace};
use agentdoctor_report::{render_json, render_pretty};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root should exist")
}

fn scan_fixture(name: &str) -> agentdoctor_engine::AuditReport {
    scan_workspace(ScanInput {
        root: repo_root().join("fixtures").join(name),
        profile: AgentProfile::default(),
        project_config: None,
        options: ScanOptions::default(),
    })
    .expect("fixture should scan")
}

#[test]
fn json_report_preserves_machine_readable_fixture_contract() {
    let report = scan_fixture("rust-cli");
    let json = render_json(&report).expect("json should render");
    let value: serde_json::Value = serde_json::from_str(&json).expect("json should parse");

    assert_eq!(value["score"]["total"], 79);
    assert_eq!(value["findings"][0]["id"], "AD009");
    assert_eq!(value["findings"][1]["id"], "AD010");
    assert_eq!(value["facts"]["commands"]["test"], "cargo test");
    assert!(!json.contains("\u{1b}["));
}

#[test]
fn pretty_report_groups_empty_repo_findings_by_severity() {
    let report = scan_fixture("empty-repo");
    let pretty = render_pretty(&report);

    assert!(pretty.contains("AI Agent Readiness: 12/100"));
    assert!(
        pretty
            .contains("Critical\n  x AD001 Missing AGENTS.md\n  x AD003 No test command detected")
    );
    assert!(pretty.contains(
        "Warnings\n  ! AD004 No build command detected\n  ! AD005 Missing boundaries section"
    ));
    assert!(pretty.contains("Info\n  i AD009 Missing README\n  i AD010 Missing CI"));
    assert!(pretty.contains("Score breakdown"));
}

#[test]
fn pretty_report_for_clean_findings_does_not_render_empty_severity_sections() {
    let report = scan_fixture("js-next");
    let pretty = render_pretty(&report);

    assert!(!pretty.contains("Critical\n"));
    assert!(!pretty.contains("Warnings\n"));
    assert!(pretty.contains("Info\n  i AD009 Missing README\n  i AD010 Missing CI"));
}
