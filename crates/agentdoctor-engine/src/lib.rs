#![forbid(unsafe_code)]

mod agents;
mod error;
mod fs;
mod generators;
mod rules;
mod score;
mod types;

pub use agents::{agent_specs, required_agent_paths};
pub use error::EngineError;
pub use types::*;

pub fn scan_workspace(input: ScanInput) -> Result<AuditReport, EngineError> {
    let mut facts = fs::discover_workspace(&input.root, &input.options)?;
    if let Some(project_config) = input.project_config.as_ref() {
        facts
            .commands
            .apply_project_commands(&project_config.commands);
    }

    let context = rules::RuleContext {
        facts: &facts,
        profile: &input.profile,
        project_config: input.project_config.as_ref(),
    };
    let findings = rules::run_rules(&context);
    let score = score::calculate_score(&context);
    let evidence = score
        .categories
        .iter()
        .flat_map(|category| category.evidence.iter().cloned())
        .collect();
    let recommendations = rules::recommendations_for(&findings);

    Ok(AuditReport {
        score,
        facts,
        findings,
        recommendations,
        evidence,
    })
}

pub fn generate_init_plan(input: InitInput) -> Result<InitPlan, EngineError> {
    generators::generate_init_plan(input)
}
