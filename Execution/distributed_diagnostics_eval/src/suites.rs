use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;

use crate::config::SuitesSettings;
use crate::summary::{validate_supported_suite_subset, SummaryError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuiteApplicability {
    InitialOnly,
    ContinuationOnly,
    Shared,
}

impl SuiteApplicability {
    pub fn applies_to_initial(self) -> bool {
        matches!(self, Self::InitialOnly | Self::Shared)
    }

    pub fn applies_to_continuation(self) -> bool {
        matches!(self, Self::ContinuationOnly | Self::Shared)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct JudgeSuiteDefinition {
    pub id: String,
    pub version: String,
    pub category: String,
    pub scope: String,
    pub applies_to: SuiteApplicability,
    pub required_for_mvp: bool,
    pub input_variables: Vec<String>,
    pub prompt_template: String,
    pub normalized_output_schema_hint: serde_json::Value,
    pub response_schema: serde_json::Value,
    #[serde(default)]
    pub what_it_checks: String,
    #[serde(default)]
    pub why_it_matters: String,
    #[serde(default)]
    pub inputs_to_judge: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct JudgeSuiteCatalog {
    pub judge_suites: BTreeMap<String, JudgeSuiteDefinition>,
}

#[derive(Debug, thiserror::Error)]
pub enum SuiteCatalogError {
    #[error("failed to read suite catalog: {0}")]
    Read(String),
    #[error("failed to parse suite catalog: {0}")]
    Parse(String),
    #[error("invalid suite selection: {0}")]
    InvalidSelection(String),
    #[error(transparent)]
    Summary(#[from] SummaryError),
}

impl JudgeSuiteCatalog {
    pub fn load_from_path(path: &Path) -> Result<Self, SuiteCatalogError> {
        let raw = std::fs::read_to_string(path)
            .map_err(|e| SuiteCatalogError::Read(e.to_string()))?;
        serde_json::from_str(&raw).map_err(|e| SuiteCatalogError::Parse(e.to_string()))
    }

    pub fn resolve_enabled_suite_names(
        &self,
        settings: &SuitesSettings,
    ) -> Result<Vec<String>, SuiteCatalogError> {
        if let Some(enabled) = &settings.enabled {
            for suite_name in enabled {
                if !self.judge_suites.contains_key(suite_name) {
                    return Err(SuiteCatalogError::InvalidSelection(format!(
                        "enabled suite not found in catalog: {suite_name}"
                    )));
                }
            }
            return Ok(enabled.clone());
        }

        let selected: Vec<String> = self
            .judge_suites
            .iter()
            .filter(|(_, def)| !settings.required_for_mvp_only || def.required_for_mvp)
            .map(|(name, _)| name.clone())
            .collect();

        if selected.is_empty() {
            return Err(SuiteCatalogError::InvalidSelection(
                "no suites selected after applying config filters".to_string(),
            ));
        }
        validate_supported_suite_subset(&selected)?;
        Ok(selected)
    }

    pub fn get(
        &self,
        suite_name: &str,
    ) -> Option<&JudgeSuiteDefinition> {
        self.judge_suites.get(suite_name)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use crate::config::SuitesSettings;
    use crate::suites::{JudgeSuiteCatalog, SuiteCatalogError};

    fn write_catalog() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempdir().unwrap();
        let path = dir.path().join("prompts.json");
        fs::write(
            &path,
            r#"
{
  "judge_suites": {
    "final_no_root_cause_claim": {
      "id": "a",
      "version": "v1",
      "category": "final_answer",
      "scope": "iteration",
      "applies_to": "shared",
      "required_for_mvp": true,
      "input_variables": ["final_answer"],
      "prompt_template": "x",
      "normalized_output_schema_hint": {"required":["score"]},
      "response_schema": {"type":"object","properties":{"score":{"type":"integer"}},"required":["score"]}
    },
    "optional_suite": {
      "id": "b",
      "version": "v1",
      "category": "final_answer",
      "scope": "iteration",
      "applies_to": "shared",
      "required_for_mvp": false,
      "input_variables": ["final_answer"],
      "prompt_template": "y",
      "normalized_output_schema_hint": {"required":["score"]},
      "response_schema": {"type":"object","properties":{"score":{"type":"integer"}},"required":["score"]}
    }
  }
}
"#,
        )
        .unwrap();
        (dir, path)
    }

    #[test]
    fn loads_catalog_and_selects_required_for_mvp() {
        let (_dir, path) = write_catalog();
        let catalog = JudgeSuiteCatalog::load_from_path(&path).unwrap();
        let selected = catalog
            .resolve_enabled_suite_names(&SuitesSettings {
                catalog_path: path,
                required_for_mvp_only: true,
                enabled: None,
            })
            .unwrap();
        assert_eq!(selected, vec!["final_no_root_cause_claim".to_string()]);
    }

    #[test]
    fn explicit_enabled_suites_are_validated() {
        let (_dir, path) = write_catalog();
        let catalog = JudgeSuiteCatalog::load_from_path(&path).unwrap();
        let err = catalog
            .resolve_enabled_suite_names(&SuitesSettings {
                catalog_path: path,
                required_for_mvp_only: false,
                enabled: Some(vec!["missing_suite".to_string()]),
            })
            .unwrap_err();
        assert!(matches!(err, SuiteCatalogError::InvalidSelection(_)));
    }
}
