//! The YAML file `stormix run` and `stormix load` read.

use serde::{Deserialize, Serialize};

use crate::{Flow, LoadProfile, Threshold};

/// The only format version so far. A file with a newer one is refused rather
/// than half-read.
pub const CURRENT_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(deny_unknown_fields)]
pub struct TestFile {
    pub version: u32,
    pub flow: Flow,
    /// Without a load profile the flow runs once, as a functional test.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub load: Option<LoadProfile>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "specta", specta(type = Vec<String>))]
    pub thresholds: Vec<Threshold>,
}

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("{0}")]
    Yaml(#[from] serde_yaml_ng::Error),
    /// The YAML is well formed but does not match the format, at `path`.
    #[error("{path}: {message}")]
    Schema { path: String, message: String },
    #[error("unsupported test file version {0}; this Stormix reads version {CURRENT_VERSION}")]
    UnsupportedVersion(u32),
}

impl TestFile {
    pub fn from_yaml(text: &str) -> Result<Self, ParseError> {
        // serde_yaml_ng only reads enums written as YAML tags (`!json {...}`).
        // Going through a JSON value lets people write the plain map form
        // (`json: {...}`) that every other YAML tool uses.
        let value: serde_json::Value = serde_yaml_ng::from_str(text)?;
        let file: Self =
            serde_path_to_error::deserialize(value).map_err(|err| ParseError::Schema {
                path: err.path().to_string(),
                message: err.inner().to_string(),
            })?;
        if file.version != CURRENT_VERSION {
            return Err(ParseError::UnsupportedVersion(file.version));
        }
        Ok(file)
    }

    pub fn to_yaml(&self) -> String {
        let value = serde_json::to_value(self).expect("a test file always serializes");
        serde_yaml_ng::to_string(&value).expect("a JSON value always serializes")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Assertion, Body, Duration, ExtractSource, Method};

    const LOGIN: &str = r#"
version: 1
flow:
  name: Login then fetch the profile
  variables:
    base_url: https://api.example.com
  steps:
    - name: Log in
      request:
        method: POST
        url: "{{base_url}}/login"
        headers:
          - { name: Accept, value: application/json }
        body:
          json: { user: demo, password: "{{password}}" }
      extract:
        token: { json: $.token }
      assert:
        - status: 200
        - max_duration_ms: 500
    - name: Profile
      request:
        method: GET
        url: "{{base_url}}/me"
        headers:
          - { name: Authorization, value: "Bearer {{token}}" }
      assert:
        - json: { path: $.user, equals: demo }
load:
  ramping:
    stages:
      - { duration: 30s, target: 50 }
      - { duration: 2m, target: 50 }
thresholds:
  - p95 < 300ms
  - errors < 1%
"#;

    #[test]
    fn reads_a_full_file() {
        let file = TestFile::from_yaml(LOGIN).unwrap();
        assert_eq!(file.flow.steps.len(), 2);
        let login = &file.flow.steps[0];
        assert_eq!(login.request.method, Method::Post);
        assert!(matches!(login.request.body, Some(Body::Json(_))));
        assert_eq!(
            login.extract["token"],
            ExtractSource::Json("$.token".into())
        );
        assert_eq!(login.assertions[0], Assertion::Status(200));
        assert_eq!(
            file.load.unwrap().total_duration(),
            Duration::from_secs(150)
        );
        assert_eq!(file.thresholds.len(), 2);
    }

    #[test]
    fn round_trips() {
        let file = TestFile::from_yaml(LOGIN).unwrap();
        assert_eq!(TestFile::from_yaml(&file.to_yaml()).unwrap(), file);
    }

    #[test]
    fn steps_are_enabled_unless_they_say_otherwise() {
        let yaml = LOGIN.replace(
            "    - name: Profile\n",
            "    - name: Profile\n      enabled: false\n",
        );
        let file = TestFile::from_yaml(&yaml).unwrap();
        assert!(file.flow.steps[0].enabled);
        assert!(!file.flow.steps[1].enabled);
        assert_eq!(file.flow.enabled_steps().count(), 1);
        // `enabled: true` is the default, so only disabled steps write it.
        let written = file.to_yaml();
        assert_eq!(written.matches("enabled:").count(), 1, "{written}");
        assert_eq!(TestFile::from_yaml(&written).unwrap(), file);
    }

    #[test]
    fn set_aside_steps_round_trip() {
        let mut file = TestFile::from_yaml(LOGIN).unwrap();
        let parked = file.flow.steps.pop().unwrap();
        file.flow.set_aside.push(parked);
        let written = file.to_yaml();
        assert!(written.contains("set_aside:"), "{written}");
        assert_eq!(TestFile::from_yaml(&written).unwrap(), file);
        assert!(!TestFile::from_yaml(LOGIN)
            .unwrap()
            .to_yaml()
            .contains("set_aside"));
    }

    #[test]
    fn refuses_other_versions_and_unknown_keys() {
        let newer = LOGIN.replace("version: 1", "version: 2");
        assert!(matches!(
            TestFile::from_yaml(&newer),
            Err(ParseError::UnsupportedVersion(2))
        ));
        let typo = LOGIN.replace("thresholds:", "threshold:");
        assert!(TestFile::from_yaml(&typo).is_err());
    }

    #[test]
    fn schema_errors_name_the_path() {
        let bad = LOGIN.replace("method: GET", "method: FETCH");
        let err = TestFile::from_yaml(&bad).unwrap_err().to_string();
        assert!(err.starts_with("flow.steps[1].request.method:"), "{err}");
    }
}
