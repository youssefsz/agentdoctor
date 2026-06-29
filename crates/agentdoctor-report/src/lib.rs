#![forbid(unsafe_code)]

use agentdoctor_engine::{AuditReport, Severity};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ReportError {
    #[error("failed to serialize JSON report: {0}")]
    Json(#[from] serde_json::Error),
}

pub fn render_json(report: &AuditReport) -> Result<String, ReportError> {
    Ok(serde_json::to_string_pretty(report)?)
}

pub fn render_pretty(report: &AuditReport) -> String {
    let mut output = String::new();
    output.push_str("AgentDoctor 0.1.0\n\n");
    output.push_str("Detected: ");
    let stack = report.facts.detected_stack_labels();
    if stack.is_empty() {
        output.push_str("No stack detected");
    } else {
        output.push_str(&stack.join(", "));
    }
    output.push_str("\n\n");
    output.push_str(&format!(
        "AI Agent Readiness: {}/100\n\n",
        report.score.total
    ));

    for severity in [
        Severity::Critical,
        Severity::Warning,
        Severity::Info,
        Severity::Suggestion,
    ] {
        let group: Vec<_> = report
            .findings
            .iter()
            .filter(|finding| finding.severity == severity)
            .collect();
        if group.is_empty() {
            continue;
        }
        output.push_str(severity.heading());
        output.push('\n');
        for finding in group {
            let marker = match severity {
                Severity::Critical => "x",
                Severity::Warning => "!",
                Severity::Info => "i",
                Severity::Suggestion => "-",
            };
            output.push_str("  ");
            output.push_str(marker);
            output.push(' ');
            output.push_str(&finding.id);
            output.push(' ');
            output.push_str(&finding.title);
            output.push('\n');
        }
        output.push('\n');
    }

    if report.findings.is_empty() {
        output.push_str("No findings.\n\n");
    }

    output.push_str("Score breakdown\n");
    for category in &report.score.categories {
        output.push_str(&format!(
            "  - {}: {}/{}\n",
            category.name, category.earned, category.max
        ));
    }

    if report.findings.is_empty() && report.score.total < 100 {
        let notes = score_notes(report);
        if !notes.is_empty() {
            output.push_str("\nScore notes\n");
            for (subject, message) in notes {
                output.push_str("  - ");
                output.push_str(subject);
                output.push_str(": ");
                output.push_str(message);
                output.push('\n');
            }
        }
    }

    if !report.recommendations.is_empty() {
        output.push_str("\nNext steps\n");
        for recommendation in &report.recommendations {
            output.push_str("  - ");
            output.push_str(&recommendation.message);
            output.push('\n');
        }
    }

    output
}

fn score_notes(report: &AuditReport) -> Vec<(&str, &str)> {
    report
        .score
        .categories
        .iter()
        .filter(|category| category.earned < category.max)
        .flat_map(|category| category.evidence.iter())
        .filter(|evidence| evidence.points == 0)
        .take(5)
        .map(|evidence| (evidence.subject.as_str(), evidence.message.as_str()))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use agentdoctor_engine::{
        AuditReport, CiFacts, CommandFacts, EnvFacts, Evidence, Score, ScoreCategory,
        WorkspaceFacts,
    };

    use super::*;

    #[test]
    fn json_report_is_valid_json() {
        let report = empty_report();
        let json = render_json(&report).expect("json");
        let value: serde_json::Value = serde_json::from_str(&json).expect("valid json");
        assert_eq!(value["score"]["total"], 0);
    }

    #[test]
    fn pretty_report_contains_score() {
        let report = empty_report();
        let pretty = render_pretty(&report);
        assert!(pretty.contains("AI Agent Readiness: 0/100"));
    }

    #[test]
    fn pretty_report_explains_clean_non_perfect_scores() {
        let mut report = empty_report();
        report.score = Score {
            total: 92,
            categories: vec![ScoreCategory {
                name: "Commands".to_string(),
                earned: 12,
                max: 20,
                evidence: vec![
                    Evidence::missed("install-command", "Command was not detected."),
                    Evidence::missed("dev-command", "Command was not detected."),
                    Evidence::awarded("build-command", 4, "Detected command: cargo build."),
                ],
            }],
        };

        let pretty = render_pretty(&report);

        assert!(pretty.contains("No findings."));
        assert!(pretty.contains("Score notes"));
        assert!(pretty.contains("  - install-command: Command was not detected."));
        assert!(pretty.contains("  - dev-command: Command was not detected."));
        assert!(!pretty.contains("build-command: Detected command"));
    }

    fn empty_report() -> AuditReport {
        AuditReport {
            score: Score {
                total: 0,
                categories: Vec::new(),
            },
            facts: WorkspaceFacts {
                root: PathBuf::from("."),
                files: Vec::new(),
                agent_files: Vec::new(),
                customizations: Default::default(),
                package_managers: Vec::new(),
                frameworks: Vec::new(),
                commands: CommandFacts::default(),
                ci: CiFacts::default(),
                env: EnvFacts::default(),
            },
            findings: Vec::new(),
            recommendations: Vec::new(),
            evidence: Vec::new(),
        }
    }
}
