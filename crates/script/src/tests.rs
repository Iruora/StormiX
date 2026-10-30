use super::*;

fn request() -> ScriptRequest {
    ScriptRequest {
        method: "GET".into(),
        url: "https://api.test/users/7".into(),
        headers: vec![("Accept".into(), "application/json".into())],
        body: None,
    }
}

fn response(status: u16, body: &str) -> ScriptResponse {
    ScriptResponse {
        status,
        status_text: if status == 200 { "OK".into() } else { "Not Found".into() },
        headers: vec![
            ("Content-Type".into(), "application/json".into()),
            ("Set-Cookie".into(), "a=1".into()),
        ],
        body: body.into(),
        time_ms: 120.0,
    }
}

const USER: &str = r#"{"id":7,"name":"Ada","tags":["admin","qa"],"profile":{"active":true},"empty":[]}"#;

fn vars() -> BTreeMap<String, String> {
    BTreeMap::from([("base".into(), "https://api.test".into()), ("n".into(), "1".into())])
}

fn pre(source: &str) -> ScriptOutput {
    let vars = vars();
    let req = request();
    run(
        source,
        &ScriptInput {
            phase: Phase::PreRequest,
            step_name: "Get user",
            variables: &vars,
            request: &req,
            response: None,
        },
        Duration::from_secs(1),
    )
}

fn test_with(source: &str, res: &ScriptResponse) -> ScriptOutput {
    let vars = vars();
    let req = request();
    run(
        source,
        &ScriptInput {
            phase: Phase::Test,
            step_name: "Get user",
            variables: &vars,
            request: &req,
            response: Some(res),
        },
        Duration::from_secs(1),
    )
}

fn test(source: &str) -> ScriptOutput {
    test_with(source, &response(200, USER))
}

/// (name, passed, message) for each test.
fn outcomes(out: &ScriptOutput) -> Vec<(String, bool, Option<String>)> {
    out.tests
        .iter()
        .map(|t| (t.name.clone(), t.passed, t.message.clone()))
        .collect()
}

#[test]
fn pre_request_sets_variables_and_headers() {
    let out = pre(r#"
        pm.environment.set("token", "abc");
        pm.variables.set("count", Number(pm.variables.get("n")) + 1);
        pm.collectionVariables.unset("n");
        pm.request.headers.add({ key: "X-Trace", value: pm.variables.get("token") });
        pm.request.headers.upsert({ key: "accept", value: "text/plain" });
        console.log("base is", pm.variables.get("base"), { ok: true });
    "#);
    assert_eq!(out.error, None);
    assert_eq!(
        out.variables,
        BTreeMap::from([
            ("count".into(), Some("2".into())),
            ("n".into(), None),
            ("token".into(), Some("abc".into())),
        ])
    );
    assert_eq!(
        out.request.headers,
        [
            ("Accept".into(), "text/plain".into()),
            ("X-Trace".into(), "abc".into())
        ]
    );
    assert_eq!(out.logs, [r#"base is https://api.test {"ok":true}"#]);
}

#[test]
fn bruno_request_helpers() {
    let out = pre(r#"
        bru.setVar("id", 42);
        req.setHeader("Authorization", "Bearer " + bru.getVar("id"));
        req.setUrl(req.getUrl() + "?v=2");
        req.setBody({ a: 1 });
        req.setMethod("post");
    "#);
    assert_eq!(out.error, None);
    assert_eq!(out.variables["id"], Some("42".into()));
    assert_eq!(out.request.url, "https://api.test/users/7?v=2");
    assert_eq!(out.request.method, "POST");
    assert_eq!(out.request.body.as_deref(), Some(r#"{"a":1}"#));
    assert!(out.request.headers.contains(&("Authorization".into(), "Bearer 42".into())));
}

#[test]
fn postman_tests_and_chai_expect() {
    let out = test(r#"
        pm.test("status", () => pm.response.to.have.status(200));
        pm.test("ok", () => { pm.response.to.be.ok; pm.response.to.be.json; });
        pm.test("header", () => pm.response.to.have.header("content-type"));
        pm.test("body", () => {
            const user = pm.response.json();
            pm.expect(user.name).to.equal("Ada");
            pm.expect(user).to.have.property("profile").that.deep.equals({ active: true });
            pm.expect(user.tags).to.be.an("array").that.includes("qa");
            pm.expect(user.tags).to.have.lengthOf(2);
            pm.expect(user.empty).to.be.empty;
            pm.expect(user.id).to.be.above(5).and.below(10);
            pm.expect(user.name).to.match(/^A/);
            pm.expect(user).to.include({ id: 7 });
            pm.expect(user).to.have.keys("id", "name", "tags", "profile", "empty");
            pm.expect(user.missing).to.not.exist;
            pm.expect(pm.response.responseTime).to.be.below(500);
            pm.expect(pm.response.headers.get("CONTENT-TYPE")).to.include("json");
        });
        pm.test("fails", () => pm.expect(pm.response.json().name).to.eql("Bob"));
        pm.test("wrong status", () => pm.response.to.have.status(201));
        pm.environment.set("userId", pm.response.json().id);
    "#);
    assert_eq!(out.error, None);
    assert_eq!(
        outcomes(&out),
        [
            ("status".into(), true, None),
            ("ok".into(), true, None),
            ("header".into(), true, None),
            ("body".into(), true, None),
            (
                "fails".into(),
                false,
                Some(r#"expected "Ada" to deeply equal "Bob""#.into())
            ),
            (
                "wrong status".into(),
                false,
                Some("expected response to have status 201 but got 200".into())
            ),
        ]
    );
    assert_eq!(out.variables["userId"], Some("7".into()));
}

#[test]
fn legacy_postman_globals() {
    let out = test(r#"
        tests["Status code is 200"] = responseCode.code === 200;
        tests["Has Ada"] = responseBody.has ? false : responseBody.indexOf("Ada") >= 0;
        tests["Is slow"] = responseTime > 1000;
        var data = JSON.parse(responseBody);
        postman.setEnvironmentVariable("name", data.name);
    "#);
    assert_eq!(out.error, None);
    assert_eq!(
        outcomes(&out),
        [
            ("Status code is 200".into(), true, None),
            ("Has Ada".into(), true, None),
            ("Is slow".into(), false, Some("was falsy".into())),
        ]
    );
    assert_eq!(out.variables["name"], Some("Ada".into()));
}

#[test]
fn bruno_tests_and_res() {
    let out = test(r#"
        test("status", () => expect(res.getStatus()).to.equal(200));
        test("body", () => {
            expect(res.body.name).to.equal("Ada");
            expect(res.getHeader("content-type")).to.contain("json");
        });
        bru.setVar("first_tag", res.body.tags[0]);
    "#);
    assert_eq!(out.error, None);
    assert!(out.tests.iter().all(|t| t.passed), "{:?}", out.tests);
    assert_eq!(out.variables["first_tag"], Some("admin".into()));
}

#[test]
fn bruno_assert_operators() {
    let out = test(r#"
        test("1", () => __bruAssert(res.status, "eq", "200"));
        test("2", () => __bruAssert(res.body.name, "eq", "Ada"));
        test("3", () => __bruAssert(res.body.name, "eq", "'Ada'"));
        test("4", () => __bruAssert(res.body.id, "gt", "5"));
        test("5", () => __bruAssert(res.body.tags, "contains", "qa"));
        test("6", () => __bruAssert(res.body.tags, "length", "2"));
        test("7", () => __bruAssert(res.body.name, "matches", "^A"));
        test("8", () => __bruAssert(res.body.missing, "isUndefined", ""));
        test("9", () => __bruAssert(res.body.profile.active, "isTruthy", ""));
        test("10", () => __bruAssert(res.body.id, "in", "6, 7"));
        test("11", () => __bruAssert(res.body.id, "between", "1, 10"));
        test("12", () => __bruAssert(res.body.name, "neq", "Ada"));
    "#);
    let failed: Vec<_> = out.tests.iter().filter(|t| !t.passed).map(|t| t.name.as_str()).collect();
    assert_eq!(failed, ["12"], "{:?}", out.tests);
}

#[test]
fn errors_are_reported_and_earlier_tests_kept() {
    let out = test(r#"
        pm.test("before", () => {});
        undefinedFunction();
    "#);
    assert_eq!(outcomes(&out), [("before".into(), true, None)]);
    assert_eq!(
        out.error.as_deref(),
        Some("ReferenceError: undefinedFunction is not defined")
    );

    let out = test("this is not javascript");
    assert!(out.error.unwrap().starts_with("SyntaxError"));

    let out = test(r#"pm.sendRequest("https://x.test", () => {});"#);
    assert_eq!(
        out.error.as_deref(),
        Some("pm.sendRequest is not supported by Stormix yet")
    );
}

#[test]
fn json_on_a_non_json_body_fails_the_test_not_the_run() {
    let out = test_with(
        r#"pm.test("json", () => pm.response.json());"#,
        &response(404, "<html>"),
    );
    assert_eq!(out.error, None);
    assert!(!out.tests[0].passed);
}

#[test]
fn runaway_scripts_are_stopped() {
    let vars = vars();
    let req = request();
    let started = Instant::now();
    let out = run(
        "while (true) {}",
        &ScriptInput {
            phase: Phase::PreRequest,
            step_name: "loop",
            variables: &vars,
            request: &req,
            response: None,
        },
        Duration::from_millis(100),
    );
    assert!(started.elapsed() < Duration::from_secs(2));
    assert_eq!(
        out.error.as_deref(),
        Some("the script did not finish within 100 ms")
    );
}

#[test]
fn no_io_is_reachable() {
    let out = pre(r#"
        pm.variables.set("types", [typeof require, typeof fetch, typeof XMLHttpRequest, typeof process].join(","));
    "#);
    assert_eq!(
        out.variables["types"],
        Some("undefined,undefined,undefined,undefined".into())
    );
}
