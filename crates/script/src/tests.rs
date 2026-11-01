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
        status_text: if status == 200 {
            "OK".into()
        } else {
            "Not Found".into()
        },
        headers: vec![
            ("Content-Type".into(), "application/json".into()),
            ("Set-Cookie".into(), "a=1".into()),
        ],
        body: body.into(),
        time_ms: 120.0,
    }
}

const USER: &str =
    r#"{"id":7,"name":"Ada","tags":["admin","qa"],"profile":{"active":true},"empty":[]}"#;

fn vars() -> BTreeMap<String, String> {
    BTreeMap::from([
        ("base".into(), "https://api.test".into()),
        ("n".into(), "1".into()),
    ])
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
    assert!(out
        .request
        .headers
        .contains(&("Authorization".into(), "Bearer 42".into())));
}

#[test]
fn postman_tests_and_chai_expect() {
    let out = test(
        r#"
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
    "#,
    );
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
    let out = test(
        r#"
        tests["Status code is 200"] = responseCode.code === 200;
        tests["Has Ada"] = responseBody.has ? false : responseBody.indexOf("Ada") >= 0;
        tests["Is slow"] = responseTime > 1000;
        var data = JSON.parse(responseBody);
        postman.setEnvironmentVariable("name", data.name);
    "#,
    );
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
    let out = test(
        r#"
        test("status", () => expect(res.getStatus()).to.equal(200));
        test("body", () => {
            expect(res.body.name).to.equal("Ada");
            expect(res.getHeader("content-type")).to.contain("json");
        });
        bru.setVar("first_tag", res.body.tags[0]);
    "#,
    );
    assert_eq!(out.error, None);
    assert!(out.tests.iter().all(|t| t.passed), "{:?}", out.tests);
    assert_eq!(out.variables["first_tag"], Some("admin".into()));
}

#[test]
fn bruno_assert_operators() {
    let out = test(
        r#"
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
    "#,
    );
    let failed: Vec<_> = out
        .tests
        .iter()
        .filter(|t| !t.passed)
        .map(|t| t.name.as_str())
        .collect();
    assert_eq!(failed, ["12"], "{:?}", out.tests);
}

#[test]
fn errors_are_reported_and_earlier_tests_kept() {
    let out = test(
        r#"
        pm.test("before", () => {});
        undefinedFunction();
    "#,
    );
    assert_eq!(outcomes(&out), [("before".into(), true, None)]);
    assert_eq!(
        out.error.as_deref(),
        Some("ReferenceError: undefinedFunction is not defined")
    );

    let out = test("this is not javascript");
    assert!(out.error.unwrap().starts_with("SyntaxError"));

    let out = test(r#"await pm.sendRequest("https://x.test");"#);
    assert_eq!(
        out.error.as_deref(),
        Some("sending requests from scripts is not available here")
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

#[test]
fn base64_helpers() {
    let out = pre(r#"
        pm.variables.set("a", btoa("demo:secret"));
        pm.variables.set("b", atob(btoa("ab")) + atob(btoa("abcd")));
    "#);
    assert_eq!(out.error, None);
    assert_eq!(out.variables["a"], Some("ZGVtbzpzZWNyZXQ=".into()));
    assert_eq!(out.variables["b"], Some("ababcd".into()));
}

#[test]
fn top_level_await_works_like_bruno() {
    let res = response(200, USER);
    let out = test_with(
        "const name = await Promise.resolve(res.getBody().name)\nbru.setVar('who', name)\ntest('async test', async () => {\n  expect(await Promise.resolve(1)).to.equal(1)\n})",
        &res,
    );
    assert_eq!(out.error, None);
    assert_eq!(out.variables.get("who"), Some(&Some("Ada".into())));
}

#[test]
fn rejected_await_is_the_script_error() {
    let res = response(200, USER);
    let out = test_with(
        "bru.setVar('before', '1')\nawait Promise.reject(new TypeError('nope'))\nbru.setVar('after', '1')",
        &res,
    );
    assert_eq!(out.error.as_deref(), Some("TypeError: nope"));
    assert!(out.variables.contains_key("before"));
    assert!(!out.variables.contains_key("after"));
}

#[test]
fn awaiting_forever_is_reported() {
    let out = pre("await new Promise(() => {})");
    assert_eq!(
        out.error.as_deref(),
        Some("the script awaited something that never finished")
    );
}

#[test]
fn awaiting_in_a_loop_still_times_out() {
    let out = pre("while (true) { await null }");
    assert!(out.error.unwrap().contains("did not finish within"));
}

#[test]
fn buffer_covers_jwt_style_encoding() {
    let out = pre(r#"
const toB64Url = (v) => Buffer.from(v, 'utf8').toString('base64').replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/g, '')
const fromB64Url = (v) => {
  const n = v.replace(/-/g, '+').replace(/_/g, '/')
  return Buffer.from(n + '='.repeat((4 - (n.length % 4)) % 4), 'base64').toString('utf8')
}
const header = toB64Url(JSON.stringify({ alg: 'none', typ: 'JWT' }))
pm.environment.set('header', header)
pm.environment.set('round', fromB64Url(toB64Url('héllo €')))
pm.environment.set('hex', Buffer.from('hi').toString('hex'))
pm.environment.set('url', Buffer.from('é?', 'utf8').toString('base64url'))
"#);
    assert_eq!(out.error, None);
    let get = |k: &str| out.variables.get(k).cloned().flatten().unwrap();
    assert_eq!(get("header"), "eyJhbGciOiJub25lIiwidHlwIjoiSldUIn0");
    assert_eq!(get("round"), "héllo €");
    assert_eq!(get("hex"), "6869");
    assert_eq!(get("url"), "w6k_");
}

#[test]
fn bru_sleep_waits_and_resolves() {
    let started = std::time::Instant::now();
    let out = pre("await bru.sleep(50)\nbru.setVar('done', 'yes')");
    assert_eq!(out.error, None);
    assert!(started.elapsed() >= Duration::from_millis(50));
    assert_eq!(out.variables.get("done"), Some(&Some("yes".into())));
}

#[test]
fn waiting_does_not_count_toward_the_time_limit() {
    // Three 500 ms sleeps outlast the 1 s limit, but sleeping is waiting.
    let out = pre("for (let i = 0; i < 3; i++) await bru.sleep(500)\nbru.setVar('done', 'yes')");
    assert_eq!(out.error, None);
    assert_eq!(out.variables.get("done"), Some(&Some("yes".into())));

    // Waiting on requests is not counted either.
    let (out, seen) = with_server(
        "for (let i = 0; i < 3; i++) await bru.sendRequest({ url: 'https://x.test' })\nbru.setVar('done', 'yes')",
        |_| {
            std::thread::sleep(Duration::from_millis(800));
            ScriptResponse {
                status: 200,
                status_text: "OK".into(),
                headers: vec![],
                body: String::new(),
                time_ms: 800.0,
            }
        },
    );
    assert_eq!(out.error, None);
    assert_eq!(seen.len(), 3);
}

#[test]
fn running_still_counts_between_sleeps() {
    let out = pre("while (true) { await bru.sleep(1); for (let i = 0; i < 1e5; i++) {} }");
    assert!(out.error.unwrap().contains("did not finish within"));
}

#[test]
fn waiting_has_its_own_cap() {
    let budget = Budget {
        started: std::time::Instant::now(),
        timeout: Duration::from_secs(5),
        waited_us: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(
            MAX_WAIT.as_micros() as u64 - 1_000,
        )),
    };
    assert!(!budget.spent());
    assert!(budget.wait_left() <= Duration::from_millis(1));
    let started = std::time::Instant::now();
    budget.waiting(|left| std::thread::sleep(Duration::from_secs(60).min(left)));
    assert!(started.elapsed() < Duration::from_secs(1));
    assert!(budget.spent());
}

/// Runs a test script whose requests `send` answers, recording each request.
fn with_server(
    source: &str,
    send: impl Fn(&ScriptRequest) -> ScriptResponse + Sync,
) -> (ScriptOutput, Vec<ScriptRequest>) {
    let vars = vars();
    let req = request();
    let res = response(500, "{}");
    let seen = std::sync::Mutex::new(Vec::new());
    let sender = |r: &ScriptRequest, _left: Duration| -> Result<ScriptResponse, String> {
        seen.lock().unwrap().push(r.clone());
        Ok(send(r))
    };
    let out = run_with(
        source,
        &ScriptInput {
            phase: Phase::Test,
            step_name: "Token",
            variables: &vars,
            request: &req,
            response: Some(&res),
        },
        Duration::from_secs(2),
        Some(&sender),
    );
    (out, seen.into_inner().unwrap())
}

#[test]
fn bru_send_request_retries_like_the_collection_does() {
    let calls = std::sync::atomic::AtomicU32::new(0);
    let (out, seen) = with_server(
        r#"
const url = req.getUrl()
const method = req.getMethod()
const headers = req.getHeaders()
const body = req.getBody({ raw: true })
let status = res.status
let token
for (let attempt = 2; attempt <= 4 && status >= 500; attempt += 1) {
  await bru.sleep(1)
  try {
    const r = await bru.sendRequest({ url, method, headers, data: body })
    status = r.status
    token = r.data.id_token
  } catch (e) {
    status = e.response.status
  }
}
bru.setVar('status', String(status))
bru.setVar('token', token)
"#,
        |_| {
            let n = calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let (status, body) = if n == 0 {
                (503, "busy")
            } else {
                (200, r#"{"id_token":"a.b.c"}"#)
            };
            ScriptResponse {
                status,
                status_text: String::new(),
                headers: vec![("Content-Type".into(), "application/json".into())],
                body: body.into(),
                time_ms: 1.0,
            }
        },
    );
    assert_eq!(out.error, None);
    assert_eq!(out.variables.get("status"), Some(&Some("200".into())));
    assert_eq!(out.variables.get("token"), Some(&Some("a.b.c".into())));
    assert_eq!(seen.len(), 2);
    assert_eq!(seen[0].url, "https://api.test/users/7");
    assert_eq!(seen[0].method, "GET");
    assert_eq!(
        seen[0].headers,
        [("Accept".to_string(), "application/json".to_string())]
    );
}

#[test]
fn bru_send_request_encodes_objects_as_json() {
    let (out, seen) = with_server(
        "const r = await bru.sendRequest({ url: 'https://x.test', method: 'post', data: { a: 1 } })\nbru.setVar('got', r.data)",
        |_| ScriptResponse {
            status: 201,
            status_text: "Created".into(),
            headers: vec![],
            body: "plain".into(),
            time_ms: 1.0,
        },
    );
    assert_eq!(out.error, None);
    assert_eq!(out.variables.get("got"), Some(&Some("plain".into())));
    assert_eq!(seen[0].method, "POST");
    assert_eq!(seen[0].body.as_deref(), Some(r#"{"a":1}"#));
    assert!(seen[0]
        .headers
        .contains(&("Content-Type".into(), "application/json".into())));
}

#[test]
fn pm_send_request_with_callback_and_promise() {
    let (out, seen) = with_server(
        r#"
pm.sendRequest({
  url: 'https://x.test/token',
  method: 'POST',
  header: [{ key: 'X-A', value: '1' }],
  body: { mode: 'urlencoded', urlencoded: [{ key: 'grant type', value: 'a&b' }] },
}, (err, res) => {
  pm.environment.set('cb', String(res.code) + ':' + res.json().ok)
})
const res = await pm.sendRequest('https://x.test/ping')
pm.test('ping', () => pm.expect(res.headers.get('content-type')).to.eql('application/json'))
"#,
        |_| ScriptResponse {
            status: 200,
            status_text: "OK".into(),
            headers: vec![("Content-Type".into(), "application/json".into())],
            body: r#"{"ok":true}"#.into(),
            time_ms: 1.0,
        },
    );
    assert_eq!(out.error, None);
    assert_eq!(out.variables.get("cb"), Some(&Some("200:true".into())));
    assert_eq!(outcomes(&out), [("ping".into(), true, None)]);
    assert_eq!(seen[0].body.as_deref(), Some("grant%20type=a%26b"));
    assert_eq!(seen[1].method, "GET");
}
