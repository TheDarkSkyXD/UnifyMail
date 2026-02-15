<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-02-15 | Updated: 2026-02-15 -->

# .github

## Purpose
GitHub-specific configuration: CI/CD workflows for building the application on multiple platforms and issue templates for structured bug reporting and feature requests.

## Subdirectories

| Directory | Purpose |
|-----------|---------|
| `workflows/` | GitHub Actions workflow files for CI/CD builds (see `workflows/AGENTS.md`) |
| `ISSUE_TEMPLATE/` | Issue and pull request templates |

## For AI Agents

### Working In This Directory
- Workflow files define the build and release pipeline
- Be cautious when modifying workflows — they affect CI/CD for all platforms

## Dependencies

### Internal
- `app/build/` — Build system that workflows invoke
- `package.json` — Scripts triggered by workflows

<!-- MANUAL: Any manually added notes below this line are preserved on regeneration -->
