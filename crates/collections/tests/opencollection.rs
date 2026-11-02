//! OpenCollection import of the shapes the fixture does not cover, and
//! export back out.

use std::path::{Path, PathBuf};

use stormix_collections::environment::{self, Environment};
use stormix_collections::{detect, import, opencollection, Format};
use stormix_flow::{Body, Header, TestFile};

fn example(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples")
        .join(name)
}

fn header<'a>(headers: &'a [Header], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|h| h.name.eq_ignore_ascii_case(name))
        .map(|h| h.value.as_str())
}

const BUNDLE: &str = r#"
opencollection: 1.0.0
info:
  name: Bundled
request:
  variables:
    - name: base
      value: http://localhost
    - name: off
      value: x
      disabled: true
  auth:
    type: apikey
    key: X-Key
    value: "{{key}}"
    placement: header
config:
  environments:
    - name: base
      variables:
        - name: key
          value: k1
        - name: host
          value: a
    - name: dev
      extends: base
      variables:
        - name: host
          value: b
        - name: token
          secret: true
items:
  - info: { name: Admin, type: folder, seq: 2 }
    request:
      headers:
        - { name: X-Folder, value: "1" }
      variables:
        - name: role
          value: [{ title: user, value: u }, { title: admin, value: a, selected: true }]
    items:
      - info: { name: Inherit, type: http, seq: 1 }
        http:
          method: delete
          url: "{{base}}/x?keep=1"
          auth: inherit
          params:
            - { name: keep, value: "1", type: query }
            - { name: add, value: "2", type: query }
            - { name: gone, value: "3", type: query, disabled: true }
          headers:
            - { name: X-Off, value: "1", disabled: true }
  - info: { name: First, type: http, seq: 1 }
    http:
      method: PUT
      url: "{{base}}/first"
      body:
        - title: small
          body: { type: text, data: small }
        - title: form
          selected: true
          body:
            type: form-urlencoded
            data:
              - { name: a, value: "1" }
              - { name: b, value: "2", disabled: true }
      auth: { type: oauth2 }
    runtime:
      assertions:
        - { expression: res.status, operator: in, value: "200,201" }
  - info: { name: Socket, type: websocket, seq: 3 }
    websocket: { url: "ws://x" }
"#;

#[test]
fn imports_a_bundled_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("api.yml");
    std::fs::write(&path, BUNDLE).unwrap();
    assert_eq!(detect(&path), Some(Format::OpenCollection));

    let imported = import(&path).unwrap();
    let flow = &imported.test.flow;
    assert_eq!(flow.name, "Bundled");
    assert_eq!(flow.variables.len(), 1);
    assert_eq!(flow.variables["base"], "http://localhost");

    let names: Vec<_> = flow.steps.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["First", "Admin / Inherit"]);
    let [first, inherit] = &flow.steps[..] else {
        unreachable!()
    };

    // The selected body variant; oauth2 is reported, not sent.
    assert_eq!(
        first.request.body,
        Some(Body::Form(vec![stormix_flow::KeyValue {
            key: "a".into(),
            value: "1".into()
        }]))
    );
    assert!(first
        .scripts
        .test
        .as_deref()
        .unwrap()
        .contains(r#""in", bru.interpolate("200,201")"#));

    assert_eq!(inherit.request.url, "{{base}}/x?keep=1&add=2");
    assert_eq!(header(&inherit.request.headers, "x-key"), Some("{{key}}"));
    assert_eq!(header(&inherit.request.headers, "x-folder"), Some("1"));
    assert_eq!(header(&inherit.request.headers, "x-off"), None);
    assert_eq!(
        inherit.scripts.pre_request.as_deref(),
        Some(r#"bru.setVar("role", bru.interpolate("a"));"#)
    );

    // `dev` extends `base`; its secret has no value.
    assert_eq!(imported.environments["dev"]["key"], "k1");
    assert_eq!(imported.environments["dev"]["host"], "b");
    assert_eq!(imported.environments["base"]["host"], "a");
    assert_eq!(imported.secrets["dev"], ["token"]);

    let warnings = imported.warnings.join("\n");
    assert!(
        warnings.contains("`First`: oauth2 auth is not supported"),
        "{warnings}"
    );
    assert!(
        warnings.contains("`Socket`: websocket requests are not supported"),
        "{warnings}"
    );
    assert!(warnings.contains("secret `token`"), "{warnings}");
}

#[test]
fn a_stormix_yaml_file_is_not_a_collection() {
    assert_eq!(detect(&example("demo-api.stormix.yaml")), None);
}

#[test]
fn reads_and_rewrites_environment_files() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("qa.yml");
    std::fs::write(
        &path,
        "name: QA\nvariables:\n  - name: a\n    value: '1'\n  - name: b\n    value: '2'\n    disabled: true\n  - name: s\n    secret: true\n",
    )
    .unwrap();
    let mut env = environment::read_file(&path).unwrap();
    assert_eq!(env.name, "QA");
    assert_eq!(env.variables.len(), 1);
    assert_eq!(env.secrets, ["s"]);

    env.variables.insert("c".into(), "3".into());
    environment::write_file(&path, &env).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.starts_with("name: QA\nvariables:\n"), "{text}");
    assert_eq!(environment::read_file(&path).unwrap(), env);
}

fn exported(test: &TestFile, envs: &[Environment]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let out = opencollection::to_files(test, envs);
    opencollection::write_dir(dir.path(), &out).unwrap();
    dir
}

#[test]
fn exports_a_test_file_that_imports_back() {
    let text = std::fs::read_to_string(example("demo-api.stormix.yaml")).unwrap();
    let test = TestFile::from_yaml(&text).unwrap();
    let env = Environment {
        name: "local".into(),
        variables: [("user".to_owned(), "demo".to_owned())].into(),
        secrets: vec!["password".into()],
    };

    let out = opencollection::to_files(&test, std::slice::from_ref(&env));
    let paths: Vec<_> = out
        .files
        .iter()
        .map(|(p, _)| p.to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        paths,
        [
            "opencollection.yml",
            "log-in.yml",
            "read-the-profile.yml",
            "environments/local.yml"
        ]
    );
    assert_eq!(
        out.warnings,
        ["load settings and thresholds are Stormix's own; Bruno has no place for them"]
    );
    let login = &out.files[1].1;
    assert!(login.contains("expression: res.body.token"), "{login}");
    assert!(
        login.contains("expression: res.headers[\"content-type\"]"),
        "{login}"
    );

    let dir = exported(&test, std::slice::from_ref(&env));
    let back = import(dir.path()).unwrap();
    assert_eq!(back.warnings.len(), 1, "{:?}", back.warnings); // the secret
    assert_eq!(back.environments["local"], env.variables);
    assert_eq!(back.secrets["local"], env.secrets);
    let flow = &back.test.flow;
    assert_eq!(flow.name, test.flow.name);
    assert_eq!(flow.variables, test.flow.variables);
    for (b, a) in flow.steps.iter().zip(&test.flow.steps) {
        assert_eq!(b.name, a.name);
        assert_eq!(b.request.method, a.request.method);
        assert_eq!(b.request.url, a.request.url);
    }
    let [login, profile] = &flow.steps[..] else {
        unreachable!()
    };
    assert_eq!(
        header(&login.request.headers, "content-type"),
        Some("application/json")
    );
    let Some(Body::Raw(body)) = &login.request.body else {
        panic!("{:?}", login.request.body)
    };
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(body).unwrap(),
        serde_json::json!({ "user": "{{user}}", "password": "secret" })
    );
    let checks = login.scripts.test.as_deref().unwrap();
    assert!(
        checks.contains(r#"bru.setVar("token", (res.body.token));"#),
        "{checks}"
    );
    assert!(checks.contains(r#"__bruAssert((res.status), "eq", bru.interpolate("200"))"#));
    let checks = profile.scripts.test.as_deref().unwrap();
    assert!(
        checks.contains(r#"__bruAssert((res.body.roles[0]), "eq", bru.interpolate("tester"))"#),
        "{checks}"
    );
    assert!(
        checks.contains(r#"__bruAssert((res.responseTime), "lte", bru.interpolate("1000"))"#),
        "{checks}"
    );
}

#[test]
fn exports_folders_graphql_and_a_bundle() {
    // The Bruno fixture: folders, Basic auth as a script, GraphQL.
    let bruno = import(&example("collections/bruno")).unwrap();
    let dir = exported(&bruno.test, &[]);
    assert!(dir.path().join("users/folder.yml").is_file());
    let gql = std::fs::read_to_string(dir.path().join("users/graphql-user.yml")).unwrap();
    assert!(
        gql.contains("type: graphql") && gql.contains("graphql:\n  method: POST"),
        "{gql}"
    );

    let back = import(dir.path()).unwrap();
    assert_eq!(back.warnings, Vec::<String>::new());
    let names = |t: &TestFile| {
        t.flow
            .steps
            .iter()
            .map(|s| s.name.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(names(&back.test), names(&bruno.test));
    for (b, a) in back.test.flow.steps.iter().zip(&bruno.test.flow.steps) {
        assert_eq!(b.request, a.request, "{}", a.name);
    }

    let (yaml, warnings) = opencollection::to_bundle(&bruno.test, &[]);
    assert_eq!(warnings, Vec::<String>::new());
    let file = dir.path().join("bundle.yml");
    std::fs::write(&file, yaml).unwrap();
    let bundled = import(&file).unwrap();
    assert_eq!(bundled.test.flow.steps, back.test.flow.steps);
}

#[test]
fn reports_what_bruno_cannot_hold() {
    let test = TestFile::from_yaml(
        r#"
version: 1
flow:
  name: Odd
  steps:
    - name: B
      request: { method: GET, url: "http://x/b" }
    - name: Off
      enabled: false
      request: { method: GET, url: "http://x/off" }
    - name: F / In
      cache: true
      request: { method: GET, url: "http://x/in" }
      extract:
        ids: { json: "$.items[*].id" }
    - name: A
      request: { method: GET, url: "http://x/a" }
"#,
    )
    .unwrap();
    let out = opencollection::to_files(&test, &[]);
    let w = out.warnings.join("\n");
    assert!(w.contains("`Off` is turned off"), "{w}");
    assert!(w.contains("`F / In`: response caching"), "{w}");
    assert!(w.contains("`ids` ($.items[*].id)"), "{w}");
    assert!(w.contains("different order"), "{w}");
}
