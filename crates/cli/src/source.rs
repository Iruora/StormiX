//! What `run`, `validate`, `import` and `export` read: a Stormix test file,
//! a Postman collection export, a Bruno collection folder or an
//! OpenCollection (folder or bundled file).

use std::collections::BTreeMap;
use std::path::Path;

use stormix_flow::TestFile;

pub struct Source {
    pub test: TestFile,
    /// Environments that came with a collection, by name.
    pub environments: BTreeMap<String, BTreeMap<String, String>>,
    /// Secret names of each environment, which have no value in it.
    pub secrets: BTreeMap<String, Vec<String>>,
    /// What an import could not carry over.
    pub warnings: Vec<String>,
}

pub fn load(path: &Path) -> Result<Source, String> {
    // A YAML file is a test file unless it is an OpenCollection.
    if stormix_collections::detect(path).is_none() {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        let test = TestFile::from_yaml(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        return Ok(Source {
            test,
            environments: BTreeMap::new(),
            secrets: BTreeMap::new(),
            warnings: Vec::new(),
        });
    }
    let imported = stormix_collections::import(path).map_err(|e| e.to_string())?;
    Ok(Source {
        test: imported.test,
        environments: imported.environments,
        secrets: imported.secrets,
        warnings: imported.warnings,
    })
}

pub fn is_collection(path: &Path) -> bool {
    stormix_collections::detect(path).is_some()
}
