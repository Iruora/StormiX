//! What `run`, `validate` and `import` read: a Stormix test file, a Postman
//! collection export or a Bruno collection folder.

use std::collections::BTreeMap;
use std::path::Path;

use stormix_collections::Format;
use stormix_flow::TestFile;

pub struct Source {
    pub test: TestFile,
    /// Environments that came with a collection, by name.
    pub environments: BTreeMap<String, BTreeMap<String, String>>,
    /// What an import could not carry over.
    pub warnings: Vec<String>,
}

pub fn load(path: &Path) -> Result<Source, String> {
    let is_yaml = path.extension().is_some_and(|e| e == "yaml" || e == "yml");
    if is_yaml || stormix_collections::detect(path).is_none() {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        let test = TestFile::from_yaml(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        return Ok(Source {
            test,
            environments: BTreeMap::new(),
            warnings: Vec::new(),
        });
    }
    let imported = stormix_collections::import(path).map_err(|e| e.to_string())?;
    Ok(Source {
        test: imported.test,
        environments: imported.environments,
        warnings: imported.warnings,
    })
}

pub fn is_collection(path: &Path) -> bool {
    matches!(
        stormix_collections::detect(path),
        Some(Format::Postman | Format::Bruno)
    ) && !path.extension().is_some_and(|e| e == "yaml" || e == "yml")
}
