//! Template system: loading, validation, and schema enforcement.
//!
//! Templates are TOML files in the corpus `.templates/` directory. Each template
//! defines required/optional frontmatter fields and content structure rules.
//! Notes declare which template they follow via a `template:` frontmatter field.

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use cxtvault_common::{Error, Result};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A parsed template definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Template {
    /// Template name (used in frontmatter `template:` field).
    pub name: String,
    /// Human-readable description.
    pub description: Option<String>,
    /// Fields that must be present in frontmatter.
    pub required_fields: Vec<FieldSchema>,
    /// Fields that may be present in frontmatter.
    pub optional_fields: Vec<FieldSchema>,
    /// Headings that must exist in the content body.
    pub required_sections: Vec<String>,
    /// Minimum word count for the content body.
    pub min_word_count: Option<usize>,
}

/// Schema for a frontmatter field.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldSchema {
    /// Field name (key in frontmatter).
    pub name: String,
    /// Expected type of the field value.
    pub field_type: FieldType,
    /// Allowed values (only for `Enum` type).
    pub values: Option<Vec<String>>,
}

/// Supported field types for validation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum FieldType {
    /// Free-form text.
    String,
    /// Date in YYYY-MM-DD format.
    Date,
    /// One of a fixed set of allowed values.
    Enum,
    /// An array of values.
    List,
    /// A file path reference.
    Path,
    /// An array of file path references.
    #[serde(rename = "listofpaths")]
    ListOfPaths,
    /// A numeric value.
    Number,
    /// A true/false value.
    Boolean,
}

/// A validation issue found during note checking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationIssue {
    /// How severe this issue is.
    pub severity: Severity,
    /// Human-readable description of the problem.
    pub message: String,
    /// Which frontmatter field is involved (if applicable).
    pub field: Option<String>,
}

/// Issue severity level.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// The note violates a hard requirement (field missing, wrong type, etc.).
    Error,
    /// The note has a soft issue (word count too low, etc.).
    Warning,
}

/// Result of validating a note against its template.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    /// Path of the validated note.
    pub path: String,
    /// Template name it was validated against (if any).
    pub template: Option<String>,
    /// Whether the note passed validation with no errors.
    pub valid: bool,
    /// All issues found.
    pub issues: Vec<ValidationIssue>,
}

// ---------------------------------------------------------------------------
// TOML deserialization helpers (internal)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct TemplateToml {
    template: TemplateHeader,
    #[serde(default)]
    required_fields: Vec<FieldSchemaToml>,
    #[serde(default)]
    optional_fields: Vec<FieldSchemaToml>,
    #[serde(default)]
    content: Option<ContentRules>,
}

#[derive(Deserialize)]
struct TemplateHeader {
    name: String,
    description: Option<String>,
}

#[derive(Deserialize)]
struct FieldSchemaToml {
    name: String,
    field_type: FieldType,
    #[serde(default)]
    values: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct ContentRules {
    #[serde(default)]
    required_sections: Vec<String>,
    min_word_count: Option<usize>,
}

// ---------------------------------------------------------------------------
// Implementation
// ---------------------------------------------------------------------------

impl Template {
    /// Load all templates from a directory (reads `.toml` files).
    ///
    /// If the directory does not exist, returns an empty map rather than an error.
    pub fn load_from_dir(dir: &Path) -> Result<HashMap<String, Template>> {
        let mut templates = HashMap::new();

        if !dir.exists() {
            return Ok(templates);
        }

        let entries = std::fs::read_dir(dir).map_err(|e| {
            Error::Io(std::io::Error::new(
                e.kind(),
                format!("cannot read templates directory {}: {}", dir.display(), e),
            ))
        })?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            if path.extension().and_then(|e| e.to_str()) != Some("toml") {
                continue;
            }

            let content = std::fs::read_to_string(&path).map_err(|e| {
                Error::Io(std::io::Error::new(
                    e.kind(),
                    format!("cannot read template file {}: {}", path.display(), e),
                ))
            })?;

            let parsed: TemplateToml = toml::from_str(&content).map_err(|e| {
                Error::Config(format!("invalid template file {}: {}", path.display(), e))
            })?;

            let content_rules = parsed
                .content
                .unwrap_or(ContentRules { required_sections: Vec::new(), min_word_count: None });

            let template = Template {
                name: parsed.template.name.clone(),
                description: parsed.template.description,
                required_fields: parsed
                    .required_fields
                    .into_iter()
                    .map(|f| FieldSchema {
                        name: f.name,
                        field_type: f.field_type,
                        values: f.values,
                    })
                    .collect(),
                optional_fields: parsed
                    .optional_fields
                    .into_iter()
                    .map(|f| FieldSchema {
                        name: f.name,
                        field_type: f.field_type,
                        values: f.values,
                    })
                    .collect(),
                required_sections: content_rules.required_sections,
                min_word_count: content_rules.min_word_count,
            };

            let _ = templates.insert(template.name.clone(), template);
        }

        Ok(templates)
    }

    /// Validate a document's frontmatter and content against this template.
    ///
    /// Returns a list of issues found (empty list means valid).
    pub fn validate(&self, frontmatter: &Option<Value>, content: &str) -> Vec<ValidationIssue> {
        let mut issues = Vec::new();

        // Extract frontmatter object (or treat as empty).
        let fm_obj = frontmatter.as_ref().and_then(|v| v.as_object());

        // 1. Check required fields exist and validate types.
        for field in &self.required_fields {
            let value = fm_obj.and_then(|obj| obj.get(&field.name));

            match value {
                None => {
                    issues.push(ValidationIssue {
                        severity: Severity::Error,
                        message: format!("missing required field '{}'", field.name),
                        field: Some(field.name.clone()),
                    });
                }
                Some(val) => {
                    self.validate_field_type(field, val, &mut issues);
                }
            }
        }

        // 2. Validate optional fields that are present.
        for field in &self.optional_fields {
            if let Some(val) = fm_obj.and_then(|obj| obj.get(&field.name)) {
                self.validate_field_type(field, val, &mut issues);
            }
        }

        // 3. Check required sections exist as headings.
        for section in &self.required_sections {
            if !has_section(content, section) {
                issues.push(ValidationIssue {
                    severity: Severity::Error,
                    message: format!("missing required section '{}'", section),
                    field: None,
                });
            }
        }

        // 4. Check minimum word count.
        if let Some(min_words) = self.min_word_count {
            let word_count = content.split_whitespace().count();
            if word_count < min_words {
                issues.push(ValidationIssue {
                    severity: Severity::Warning,
                    message: format!("content has {} words, minimum is {}", word_count, min_words),
                    field: None,
                });
            }
        }

        issues
    }

    /// Validate a single field value against its schema.
    fn validate_field_type(
        &self,
        field: &FieldSchema,
        value: &Value,
        issues: &mut Vec<ValidationIssue>,
    ) {
        match field.field_type {
            FieldType::String | FieldType::Path => {
                if !value.is_string() {
                    issues.push(ValidationIssue {
                        severity: Severity::Error,
                        message: format!("field '{}' must be a string", field.name),
                        field: Some(field.name.clone()),
                    });
                }
            }
            FieldType::Date => {
                if let Some(s) = value.as_str() {
                    if !is_date_like(s) {
                        issues.push(ValidationIssue {
                            severity: Severity::Error,
                            message: format!(
                                "field '{}' must be a date (YYYY-MM-DD), got '{}'",
                                field.name, s
                            ),
                            field: Some(field.name.clone()),
                        });
                    }
                } else {
                    issues.push(ValidationIssue {
                        severity: Severity::Error,
                        message: format!("field '{}' must be a date string", field.name),
                        field: Some(field.name.clone()),
                    });
                }
            }
            FieldType::Enum => {
                if let Some(s) = value.as_str() {
                    if let Some(allowed) = &field.values {
                        if !allowed.contains(&s.to_string()) {
                            issues.push(ValidationIssue {
                                severity: Severity::Error,
                                message: format!(
                                    "field '{}' has invalid value '{}', allowed: {:?}",
                                    field.name, s, allowed
                                ),
                                field: Some(field.name.clone()),
                            });
                        }
                    }
                } else {
                    issues.push(ValidationIssue {
                        severity: Severity::Error,
                        message: format!("field '{}' must be a string (enum)", field.name),
                        field: Some(field.name.clone()),
                    });
                }
            }
            FieldType::List | FieldType::ListOfPaths => {
                if !value.is_array() {
                    issues.push(ValidationIssue {
                        severity: Severity::Error,
                        message: format!("field '{}' must be an array", field.name),
                        field: Some(field.name.clone()),
                    });
                }
            }
            FieldType::Number => {
                if !value.is_number() {
                    issues.push(ValidationIssue {
                        severity: Severity::Error,
                        message: format!("field '{}' must be a number", field.name),
                        field: Some(field.name.clone()),
                    });
                }
            }
            FieldType::Boolean => {
                if !value.is_boolean() {
                    issues.push(ValidationIssue {
                        severity: Severity::Error,
                        message: format!("field '{}' must be a boolean", field.name),
                        field: Some(field.name.clone()),
                    });
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Check if a string looks like a YYYY-MM-DD date.
fn is_date_like(s: &str) -> bool {
    if s.len() < 10 {
        return false;
    }
    let bytes = s.as_bytes();
    // Check pattern: NNNN-NN-NN
    bytes[0..4].iter().all(|b| b.is_ascii_digit())
        && bytes[4] == b'-'
        && bytes[5..7].iter().all(|b| b.is_ascii_digit())
        && bytes[7] == b'-'
        && bytes[8..10].iter().all(|b| b.is_ascii_digit())
}

/// Check if content contains a heading matching the given section name.
/// Matches `# Section Name`, `## Section Name`, etc.
fn has_section(content: &str, section: &str) -> bool {
    for line in content.lines() {
        let trimmed = line.trim();
        // Strip leading `#` characters and spaces.
        if let Some(rest) = trimmed.strip_prefix('#') {
            let heading = rest.trim_start_matches('#').trim();
            if heading.eq_ignore_ascii_case(section) {
                return true;
            }
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Helper: build a template TOML string for a decision-record template.
    fn decision_record_toml() -> &'static str {
        r#"
[template]
name = "decision-record"
description = "Architecture Decision Record"

[[required_fields]]
name = "status"
field_type = "enum"
values = ["proposed", "accepted", "deprecated", "superseded"]

[[required_fields]]
name = "date"
field_type = "date"

[[optional_fields]]
name = "superseded_by"
field_type = "path"

[content]
required_sections = ["Context", "Decision", "Consequences"]
min_word_count = 50
"#
    }

    #[test]
    fn test_load_template_from_toml() {
        let tmp = TempDir::new().unwrap();
        let template_path = tmp.path().join("decision-record.toml");
        fs::write(&template_path, decision_record_toml()).unwrap();

        let templates = Template::load_from_dir(tmp.path()).unwrap();

        assert_eq!(templates.len(), 1);
        let tmpl = templates.get("decision-record").unwrap();
        assert_eq!(tmpl.name, "decision-record");
        assert_eq!(tmpl.description, Some("Architecture Decision Record".to_string()));
        assert_eq!(tmpl.required_fields.len(), 2);
        assert_eq!(tmpl.required_fields[0].name, "status");
        assert_eq!(tmpl.required_fields[0].field_type, FieldType::Enum);
        assert_eq!(
            tmpl.required_fields[0].values,
            Some(vec![
                "proposed".to_string(),
                "accepted".to_string(),
                "deprecated".to_string(),
                "superseded".to_string(),
            ])
        );
        assert_eq!(tmpl.required_fields[1].name, "date");
        assert_eq!(tmpl.required_fields[1].field_type, FieldType::Date);
        assert_eq!(tmpl.optional_fields.len(), 1);
        assert_eq!(tmpl.optional_fields[0].name, "superseded_by");
        assert_eq!(tmpl.optional_fields[0].field_type, FieldType::Path);
        assert_eq!(tmpl.required_sections, vec!["Context", "Decision", "Consequences"]);
        assert_eq!(tmpl.min_word_count, Some(50));
    }

    #[test]
    fn test_load_from_nonexistent_dir() {
        let templates = Template::load_from_dir(Path::new("/nonexistent/dir")).unwrap();
        assert!(templates.is_empty());
    }

    #[test]
    fn test_validate_valid_note() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("decision-record.toml"), decision_record_toml()).unwrap();

        let templates = Template::load_from_dir(tmp.path()).unwrap();
        let tmpl = templates.get("decision-record").unwrap();

        let frontmatter = serde_json::json!({
            "template": "decision-record",
            "status": "accepted",
            "date": "2024-01-15"
        });

        // Content with all required sections and enough words.
        let content = r#"# ADR-001

## Context

We need to choose a database for our new service. The current system uses PostgreSQL
but we are evaluating alternatives for better scalability.

## Decision

We will use PostgreSQL with read replicas for horizontal read scaling. This leverages
our existing expertise and tooling while addressing the scalability concern.

## Consequences

This means we need to set up replication infrastructure and handle eventual consistency
in read paths. The team is familiar with PostgreSQL so onboarding cost is low.
"#;

        let issues = tmpl.validate(&Some(frontmatter), content);
        assert!(issues.is_empty(), "Valid note should have no issues: {:?}", issues);
    }

    #[test]
    fn test_validate_missing_required_field() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("decision-record.toml"), decision_record_toml()).unwrap();

        let templates = Template::load_from_dir(tmp.path()).unwrap();
        let tmpl = templates.get("decision-record").unwrap();

        // Missing 'date' field.
        let frontmatter = serde_json::json!({
            "template": "decision-record",
            "status": "accepted"
        });

        let content = "## Context\n\n## Decision\n\n## Consequences\n\nEnough words here to pass the minimum word count requirement for the template validation check.\nExtra words to pad out.Extra words to pad out.Extra words to pad out.Extra words to pad out.\n";

        let issues = tmpl.validate(&Some(frontmatter), content);
        assert!(!issues.is_empty());
        let missing = issues
            .iter()
            .find(|i| i.field.as_deref() == Some("date") && i.severity == Severity::Error);
        assert!(missing.is_some(), "Should report missing 'date' field");
    }

    #[test]
    fn test_validate_invalid_enum() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("decision-record.toml"), decision_record_toml()).unwrap();

        let templates = Template::load_from_dir(tmp.path()).unwrap();
        let tmpl = templates.get("decision-record").unwrap();

        // Invalid enum value for 'status'.
        let frontmatter = serde_json::json!({
            "template": "decision-record",
            "status": "invalid-status",
            "date": "2024-01-15"
        });

        let content = "## Context\n\n## Decision\n\n## Consequences\n\nEnough words here to pass the minimum word count requirement for the template validation check.\nExtra words to pad out.Extra words to pad out.Extra words to pad out.Extra words to pad out.\n";

        let issues = tmpl.validate(&Some(frontmatter), content);
        let enum_issue = issues
            .iter()
            .find(|i| i.field.as_deref() == Some("status") && i.severity == Severity::Error);
        assert!(enum_issue.is_some(), "Should report invalid enum value: {:?}", issues);
        assert!(
            enum_issue.unwrap().message.contains("invalid-status"),
            "Error message should mention the invalid value"
        );
    }

    #[test]
    fn test_validate_missing_section() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("decision-record.toml"), decision_record_toml()).unwrap();

        let templates = Template::load_from_dir(tmp.path()).unwrap();
        let tmpl = templates.get("decision-record").unwrap();

        let frontmatter = serde_json::json!({
            "template": "decision-record",
            "status": "accepted",
            "date": "2024-01-15"
        });

        // Missing "Decision" section.
        let content = "## Context\n\nSome context about the problem we are solving and the constraints we face.\n\n## Consequences\n\nThe consequences of this decision are significant and will affect the team for a long time going forward with multiple impacts.\n";

        let issues = tmpl.validate(&Some(frontmatter), content);
        let section_issue =
            issues.iter().find(|i| i.message.contains("Decision") && i.severity == Severity::Error);
        assert!(section_issue.is_some(), "Should report missing 'Decision' section: {:?}", issues);
    }

    #[test]
    fn test_validate_word_count() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("decision-record.toml"), decision_record_toml()).unwrap();

        let templates = Template::load_from_dir(tmp.path()).unwrap();
        let tmpl = templates.get("decision-record").unwrap();

        let frontmatter = serde_json::json!({
            "template": "decision-record",
            "status": "accepted",
            "date": "2024-01-15"
        });

        // Very short content (less than 50 words).
        let content =
            "## Context\n\nShort.\n\n## Decision\n\nBrief.\n\n## Consequences\n\nMinimal.\n";

        let issues = tmpl.validate(&Some(frontmatter), content);
        let word_issue =
            issues.iter().find(|i| i.severity == Severity::Warning && i.message.contains("words"));
        assert!(word_issue.is_some(), "Should warn about insufficient word count: {:?}", issues);
    }
}
