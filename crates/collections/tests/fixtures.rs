//! Imports the fixture collections in `examples/collections`. CI also runs
//! them against the demo API, which checks the scripts themselves.

use std::path::{Path, PathBuf};

use stormix_collections::{detect, import, Format};
use stormix_flow::{Body, Header};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/collections")
        .join(name)
}

fn header<'a>(headers: &'a [Header], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|h| h.name.eq_ignore_ascii_case(name))
        .map(|h| h.value.as_str())
}

const STEPS: [&str; 6] = [
    "Health",
    "Auth / Log in",
    "Auth / Read the profile",
    "Users / Get a user",
    "Users / Admin with Basic auth",
    "Users / Echo a form",
];

#[test]
fn postman_collection() {
    let path = fixture("demo-api.postman_collection.json");
    assert_eq!(detect(&path), Some(Format::Postman));
    let imported = import(&path).unwrap();
    assert_eq!(imported.warnings, Vec::<String>::new());
    let flow = &imported.test.flow;
    assert_eq!(flow.name, "Demo API (Postman)");
    assert_eq!(flow.variables["base_url"], "http://127.0.0.1:3999");

    let names: Vec<_> = flow.steps.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, STEPS);
    let [health, login, profile, user, admin, echo] = &flow.steps[..] else {
        unreachable!()
    };

    assert_eq!(header(&health.request.headers, "authorization"), None);
    let test = health.scripts.test.as_deref().unwrap();
    assert!(
        test.starts_with("(function () {\npm.test(\"responds within 2 s\""),
        "{test}"
    );
    assert!(test.contains("tests[\"Status code is 200\"]"));

    assert_eq!(
        header(&login.request.headers, "content-type"),
        Some("application/json")
    );
    assert!(matches!(&login.request.body, Some(Body::Raw(t)) if t.contains("{{password}}")));

    // The collection's bearer auth reaches requests that inherit it.
    assert_eq!(
        header(&profile.request.headers, "authorization"),
        Some("Bearer {{token}}")
    );
    assert_eq!(user.request.url, "{{base_url}}/users/7");
    // Basic auth from variables is built by a script, after the request's own.
    assert!(admin
        .scripts
        .pre_request
        .as_deref()
        .unwrap()
        .contains("btoa(pm.variables.replaceIn(\"{{user}}\")"));
    let Some(Body::Form(fields)) = &echo.request.body else {
        panic!("{:?}", echo.request.body)
    };
    assert_eq!(fields.len(), 2);

    // Scripts set every variable the requests use that the collection does not.
    assert_eq!(imported.test.validate(), []);
}

#[test]
fn bruno_collection() {
    let path = fixture("bruno");
    assert_eq!(detect(&path), Some(Format::Bruno));
    let imported = import(&path).unwrap();
    assert_eq!(imported.warnings, Vec::<String>::new());
    assert_eq!(imported.environments["local"]["user"], "demo");
    let flow = &imported.test.flow;
    assert_eq!(flow.name, "Demo API (Bruno)");

    let names: Vec<_> = flow.steps.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, STEPS);
    let [health, login, profile, user, admin, echo] = &flow.steps[..] else {
        unreachable!()
    };
    assert_eq!(
        header(&health.request.headers, "x-client"),
        Some("stormix-fixture")
    );
    let test = health.scripts.test.as_deref().unwrap();
    assert!(
        test.contains(r#"test("res.status: eq 200", function () { __bruAssert((res.status), "eq", bru.interpolate("200")); });"#),
        "{test}"
    );

    assert_eq!(
        login.scripts.pre_request.as_deref(),
        Some(r#"bru.setVar("password", bru.interpolate("secret"));"#)
    );
    assert!(login
        .scripts
        .test
        .as_deref()
        .unwrap()
        .starts_with("(function () {\nbru.setVar(\"token\", (res.body.token));"));

    assert_eq!(
        header(&profile.request.headers, "authorization"),
        Some("Bearer {{token}}")
    );
    assert!(!profile.scripts.test.as_deref().unwrap().contains("never"));
    assert_eq!(user.request.url, "{{base_url}}/users/7");
    assert_eq!(
        header(&user.request.headers, "accept"),
        Some("application/json")
    );
    assert!(admin
        .scripts
        .pre_request
        .as_deref()
        .unwrap()
        .contains("btoa("));
    let Some(Body::Multipart(fields)) = &echo.request.body else {
        panic!("{:?}", echo.request.body)
    };
    assert_eq!(fields.len(), 2);
}

#[test]
fn bad_input_is_reported_with_its_location() {
    let dir = tempfile::tempdir().unwrap();
    assert!(import(dir.path())
        .unwrap_err()
        .to_string()
        .contains("is not a Bruno collection"));

    std::fs::write(dir.path().join("broken.bru"), "meta {\n  name: x\n").unwrap();
    let err = import(dir.path()).unwrap_err().to_string();
    assert!(err.ends_with("broken.bru:1: the `meta` block is never closed"), "{err}");

    let json = dir.path().join("old.json");
    std::fs::write(
        &json,
        r#"{"info":{"name":"x","schema":"https://schema.getpostman.com/json/collection/v1.0.0/collection.json"}}"#,
    )
    .unwrap();
    assert!(import(&json).unwrap_err().to_string().contains("Collection v2.1"));
}
