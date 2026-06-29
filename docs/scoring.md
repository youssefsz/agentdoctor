# Scoring And Findings

AgentDoctor reports two related but different things:

- Findings are actionable rule violations with stable IDs.
- Score evidence explains why points were awarded or missed.

A report can show `No findings` and still score below 100. That means no rule
violation was found, but some non-critical evidence was missing, such as an
install or dev command.

## Score Categories

The score is out of 100:

| Category | Points | What It Measures |
| --- | ---: | --- |
| Agent files | 25 | Canonical and selected agent files, plus healthy advanced customizations |
| Project-specific detail | 20 | Overview, stack, structure, and non-generic instructions |
| Commands | 20 | Install, dev, build, test, and quality commands |
| Safety boundaries | 15 | Boundaries, protected paths, and before-finish checks |
| Repo hygiene | 10 | README, environment example coverage, ignored generated dirs |
| Automation/CI | 10 | CI presence and useful CI checks |

Pretty output shows the category totals. JSON output includes full score
evidence for every category.

## Finding IDs

| ID | Severity | Meaning |
| --- | --- | --- |
| `AD001` | Critical | Missing canonical `AGENTS.md` |
| `AD002` | Warning | Missing selected agent adapter file |
| `AD003` | Critical | Missing test command |
| `AD004` | Warning | Missing build command |
| `AD005` | Warning | Missing agent boundaries section |
| `AD006` | Warning | Agent instructions are too generic |
| `AD007` | Critical | Dangerous agent instruction detected |
| `AD008` | Warning | Missing `.env.example` for detected environment usage |
| `AD009` | Info | Missing README |
| `AD010` | Info | Missing CI |
| `AD011` | Warning | Invalid skill metadata |
| `AD012` | Suggestion | Duplicate skill name |
| `AD013` | Warning | Skill pre-approves broad shell tools |
| `AD014` | Warning | Local-only agent settings are committed |
| `AD015` | Warning | MCP config may contain a secret-like literal |

## Agent Customization Checks

AgentDoctor detects and validates these agent customization surfaces when they
are present:

- `.agents/skills/*/SKILL.md`
- `.claude/skills/*/SKILL.md`
- `.github/skills/*/SKILL.md`
- `.cursor/skills/*/SKILL.md`
- `.github/instructions/*.instructions.md`
- `.github/prompts/*.prompt.md`
- `.github/agents/**/*.md`
- `.claude/commands/**/*.md`
- `.claude/agents/**/*.md`
- `.codex/agents/*.toml`
- `.mcp.json`, `.vscode/mcp.json`, `.cursor/mcp.json`, `.codex/config.toml`

Missing advanced customizations are not a finding. A repository does not need
skills, prompts, custom agents, or MCP config to be healthy. AgentDoctor reports
problems only when those files exist and look risky or malformed.

## JSON Evidence

Use JSON when you need the full audit data:

```bash
agentdoctor scan --format json --no-interactive
```

The JSON report includes:

- `score.categories[].evidence`
- `facts`
- `findings`
- `recommendations`
- flattened top-level `evidence`

AgentDoctor must not print secret values from `.env` or MCP config findings.

