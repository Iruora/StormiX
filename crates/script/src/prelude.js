// The sandbox scripts run in: Postman's `pm` (plus its legacy globals) and
// Bruno's `bru`, `req`, `res`, `test` and `expect`, over one shared variable
// store. Rust passes `__INPUT` (JSON) and reads back what `__finish()` returns.
// There is no network or file access: QuickJS has none unless given it.
"use strict";

const __input = JSON.parse(__INPUT);
const __vars = Object.assign({}, __input.variables);
const __changes = {};
const __tests = [];
const __logs = [];
const __has = (o, k) => Object.prototype.hasOwnProperty.call(o, k);

function __show(v) {
  if (typeof v === "string") return JSON.stringify(v);
  if (v === undefined) return "undefined";
  if (typeof v === "function") return "[function]";
  try {
    const s = JSON.stringify(v);
    return s === undefined ? String(v) : s.length > 200 ? s.slice(0, 197) + "..." : s;
  } catch (e) {
    return String(v);
  }
}

function __text(v) {
  if (v === undefined || v === null) return "";
  if (typeof v === "string") return v;
  if (typeof v === "object") return JSON.stringify(v);
  return String(v);
}

// ---------------------------------------------------------------- base64

// Both sandboxes offer these; collections use them for Basic auth headers.
const __B64 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
globalThis.btoa = (input) => {
  const s = String(input);
  let out = "";
  for (let i = 0; i < s.length; i += 3) {
    const [a, b, c] = [s.charCodeAt(i), s.charCodeAt(i + 1), s.charCodeAt(i + 2)];
    if (a > 255 || b > 255 || c > 255) throw new Error("btoa: the string has characters outside Latin-1");
    const n = (a << 16) | ((b || 0) << 8) | (c || 0);
    out += __B64[n >> 18] + __B64[(n >> 12) & 63];
    out += i + 1 < s.length ? __B64[(n >> 6) & 63] : "=";
    out += i + 2 < s.length ? __B64[n & 63] : "=";
  }
  return out;
};
globalThis.atob = (input) => {
  const s = String(input).replace(/[\s=]+/g, "");
  let out = "";
  for (let i = 0; i < s.length; i += 4) {
    const n = [0, 1, 2, 3].reduce((acc, j) => (acc << 6) | Math.max(__B64.indexOf(s[i + j] || "A"), 0), 0);
    out += String.fromCharCode(n >> 16);
    if (i + 2 < s.length) out += String.fromCharCode((n >> 8) & 255);
    if (i + 3 < s.length) out += String.fromCharCode(n & 255);
  }
  return out;
};

// Node's Buffer, enough for the encoding helpers Bruno scripts carry
// (base64url JWT parts, Basic auth): `Buffer.from(value, encoding)` and
// `.toString(encoding)` for utf8, base64, base64url, hex, latin1/binary and
// ascii. Bytes are held as a Latin-1 string.
class Buffer {
  constructor(bytes) {
    this._bytes = bytes;
    this.length = bytes.length;
  }
  static from(value, encoding = "utf8") {
    if (value instanceof Buffer) return new Buffer(value._bytes);
    if (Array.isArray(value) || value instanceof Uint8Array) {
      return new Buffer(Array.from(value, (b) => String.fromCharCode(b & 255)).join(""));
    }
    const s = String(value);
    switch (String(encoding).toLowerCase()) {
      case "utf8": case "utf-8": return new Buffer(unescape(encodeURIComponent(s)));
      case "base64": return new Buffer(atob(s));
      case "base64url": return new Buffer(atob(s.replace(/-/g, "+").replace(/_/g, "/")));
      case "hex": return new Buffer((s.match(/../g) || []).map((h) => String.fromCharCode(parseInt(h, 16))).join(""));
      case "latin1": case "binary": case "ascii": return new Buffer(s.replace(/[^\x00-\xff]/g, (c) => String.fromCharCode(c.charCodeAt(0) & 255)));
      default: throw new Error("Buffer: unsupported encoding " + encoding);
    }
  }
  static isBuffer(value) {
    return value instanceof Buffer;
  }
  static byteLength(value, encoding) {
    return Buffer.from(value, encoding).length;
  }
  static concat(list) {
    return new Buffer(list.map((b) => b._bytes).join(""));
  }
  toString(encoding = "utf8") {
    const b = this._bytes;
    switch (String(encoding).toLowerCase()) {
      case "utf8": case "utf-8": return decodeURIComponent(escape(b));
      case "base64": return btoa(b);
      case "base64url": return btoa(b).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
      case "hex": return Array.from(b, (c) => c.charCodeAt(0).toString(16).padStart(2, "0")).join("");
      case "latin1": case "binary": return b;
      case "ascii": return Array.from(b, (c) => String.fromCharCode(c.charCodeAt(0) & 127)).join("");
      default: throw new Error("Buffer: unsupported encoding " + encoding);
    }
  }
  toJSON() {
    return { type: "Buffer", data: Array.from(this._bytes, (c) => c.charCodeAt(0)) };
  }
}
globalThis.Buffer = Buffer;

// ---------------------------------------------------------------- sending requests

// `bru.sendRequest` and `pm.sendRequest` put the request in an outbox and
// return a promise. Rust sends what is in the outbox once the script is
// waiting, then calls `__deliver` with the reply.
let __outbox = [];
const __waiting = new Map();
let __nextCall = 1;
function __send(request) {
  return new Promise((resolve, reject) => {
    const id = __nextCall++;
    __outbox.push({ id, request });
    __waiting.set(id, { resolve, reject });
  });
}
globalThis.__takeOutbox = () => {
  const taken = __outbox;
  __outbox = [];
  return JSON.stringify(taken);
};
globalThis.__deliver = (id, json) => {
  const waiting = __waiting.get(id);
  if (!waiting) return;
  __waiting.delete(id);
  const reply = JSON.parse(json);
  if (reply.error !== undefined) waiting.reject(new Error(reply.error));
  else waiting.resolve(reply.response);
};

// Headers as an object, a Postman `[{ key, value }]` list or "Name: value"
// lines, as `[name, value]` pairs.
function __headerPairs(headers) {
  if (!headers) return [];
  if (typeof headers === "string") {
    return headers
      .split("\n")
      .map((line) => line.trim())
      .filter((line) => line.includes(":"))
      .map((line) => [line.slice(0, line.indexOf(":")).trim(), line.slice(line.indexOf(":") + 1).trim()]);
  }
  if (Array.isArray(headers)) {
    return headers.filter((h) => h && !h.disabled).map((h) => [String(h.key), __text(h.value)]);
  }
  return Object.entries(headers)
    .filter(([, v]) => v !== undefined && v !== null)
    .map(([k, v]) => [k, __text(v)]);
}
const __withHeader = (pairs, name, value) =>
  pairs.some(([k]) => k.toLowerCase() === name.toLowerCase()) ? pairs : pairs.concat([[name, value]]);
const __headerObject = (pairs) => Object.fromEntries(pairs.map(([k, v]) => [k.toLowerCase(), v]));
const __maybeJson = (text) => {
  try { return JSON.parse(text); } catch (e) { return text; }
};
const __formEncode = (pairs) =>
  pairs.map(([k, v]) => encodeURIComponent(k) + "=" + encodeURIComponent(v)).join("&");

// Bruno's, which is axios underneath: `{ url, method, headers, data }`
// resolves to `{ status, statusText, headers, data }` and rejects on a
// status outside 2xx with that response as `error.response`. An optional
// `(error, response)` callback is called as well.
function __bruSendRequest(options, callback) {
  const o = typeof options === "string" ? { url: options } : options || {};
  let headers = __headerPairs(o.headers);
  let body = o.data !== undefined ? o.data : o.body;
  if (body instanceof Buffer) {
    body = body.toString("latin1");
  } else if (body !== undefined && body !== null && typeof body !== "string") {
    body = JSON.stringify(body);
    headers = __withHeader(headers, "Content-Type", "application/json");
  }
  const promise = __send({
    method: String(o.method || "GET").toUpperCase(),
    url: String(o.url),
    headers,
    body: body === undefined || body === null ? null : String(body),
  }).then((r) => {
    const response = {
      status: r.status,
      statusText: r.statusText,
      headers: __headerObject(r.headers),
      data: __maybeJson(r.body),
      responseTime: r.timeMs,
    };
    if (r.status < 200 || r.status >= 300) {
      const error = new Error("Request failed with status code " + r.status);
      error.response = response;
      error.status = r.status;
      throw error;
    }
    return response;
  });
  if (typeof callback !== "function") return promise;
  return promise.then(
    (response) => callback(null, response),
    (error) => callback(error, error.response),
  );
}

// Postman's: a URL or `{ url, method, header, body: { mode, raw | urlencoded } }`.
// The callback gets `(error, response)`, with an error only when the request
// could not be sent; without a callback it returns a promise of the response.
function __pmSendRequest(request, callback) {
  const r = typeof request === "string" ? { url: request } : request || {};
  let headers = __headerPairs(r.header || r.headers);
  let body = null;
  const b = r.body;
  if (typeof b === "string") {
    body = b;
  } else if (b && (b.mode === "raw" || (b.mode === undefined && b.raw !== undefined))) {
    body = __text(b.raw);
  } else if (b && b.mode === "urlencoded") {
    body = __formEncode((b.urlencoded || []).filter((f) => !f.disabled).map((f) => [String(f.key), __text(f.value)]));
    headers = __withHeader(headers, "Content-Type", "application/x-www-form-urlencoded");
  } else if (b && b.mode) {
    throw new Error("pm.sendRequest: " + b.mode + " bodies are not supported yet");
  }
  const url = typeof r.url === "string" ? r.url : r.url && r.url.raw ? r.url.raw : String(r.url);
  const promise = __send({ method: String(r.method || "GET").toUpperCase(), url, headers, body }).then((res) => {
    const find = (name) => {
      const hit = res.headers.find(([k]) => k.toLowerCase() === String(name).toLowerCase());
      return hit ? hit[1] : undefined;
    };
    return {
      code: res.status,
      status: res.statusText,
      responseTime: res.timeMs,
      responseSize: res.body.length,
      headers: { get: find, has: (name) => find(name) !== undefined, toObject: () => __headerObject(res.headers) },
      text: () => res.body,
      json: () => JSON.parse(res.body),
    };
  });
  if (typeof callback !== "function") return promise;
  return promise.then(
    (response) => callback(null, response),
    (error) => callback(error, undefined),
  );
}

// ---------------------------------------------------------------- console

globalThis.console = {};
for (const level of ["log", "info", "warn", "error", "debug"]) {
  console[level] = (...args) =>
    __logs.push(args.map((a) => (typeof a === "string" ? a : __show(a))).join(" "));
}

// ---------------------------------------------------------------- variables

// Postman's scopes (globals, collection, environment, local) all read and
// write the same store; the flow has one set of variables.
const __store = {
  get: (k) => (__has(__vars, k) ? __vars[k] : undefined),
  set: (k, v) => {
    const s = __text(v);
    __vars[k] = s;
    __changes[k] = s;
  },
  unset: (k) => {
    delete __vars[k];
    __changes[k] = null;
  },
  has: (k) => __has(__vars, k),
  toObject: () => Object.assign({}, __vars),
  replaceIn: (s) =>
    String(s).replace(/\{\{\s*([^{}]+?)\s*\}\}/g, (m, n) => (__has(__vars, n) ? __vars[n] : m)),
  clear: () => Object.keys(__vars).forEach((k) => __store.unset(k)),
};

// ---------------------------------------------------------------- expect

class AssertionError extends Error {
  constructor(message) {
    super(message);
    this.name = "AssertionError";
  }
}

function __typeOf(v) {
  if (v === null) return "null";
  if (Array.isArray(v)) return "array";
  return typeof v;
}

function __deepEqual(a, b) {
  if (a === b) return true;
  if (typeof a === "number" && typeof b === "number") return Number.isNaN(a) && Number.isNaN(b);
  if (__typeOf(a) !== __typeOf(b) || typeof a !== "object" || a === null) return false;
  if (Array.isArray(a)) return a.length === b.length && a.every((x, i) => __deepEqual(x, b[i]));
  const ka = Object.keys(a);
  const kb = Object.keys(b);
  return ka.length === kb.length && ka.every((k) => __has(b, k) && __deepEqual(a[k], b[k]));
}

function Assertion(subject, message) {
  this._subject = subject;
  this._message = message;
  this._negate = false;
  this._deep = false;
}

Assertion.prototype._assert = function (ok, what, expected) {
  if (Boolean(ok) === this._negate) {
    const tail = expected === undefined ? "" : " " + __show(expected);
    const text = "expected " + __show(this._subject) + (this._negate ? " not " : " ") + "to " + what + tail;
    throw new AssertionError(this._message ? this._message + ": " + text : text);
  }
  return this;
};

for (const word of ["to", "be", "been", "is", "that", "which", "and", "has", "have", "with", "at", "of", "same", "but", "does", "still", "also", "own", "nested", "any", "all"]) {
  Object.defineProperty(Assertion.prototype, word, { get() { return this; } });
}
Object.defineProperty(Assertion.prototype, "not", { get() { this._negate = !this._negate; return this; } });
Object.defineProperty(Assertion.prototype, "deep", { get() { this._deep = true; return this; } });

const __flags = {
  ok: [(v) => Boolean(v), "be truthy"],
  true: [(v) => v === true, "be true"],
  false: [(v) => v === false, "be false"],
  null: [(v) => v === null, "be null"],
  undefined: [(v) => v === undefined, "be undefined"],
  NaN: [(v) => Number.isNaN(v), "be NaN"],
  exist: [(v) => v !== null && v !== undefined, "exist"],
  empty: [
    (v) =>
      typeof v === "string" || Array.isArray(v)
        ? v.length === 0
        : v && typeof v === "object"
          ? Object.keys(v).length === 0
          : false,
    "be empty",
  ],
};
for (const name of Object.keys(__flags)) {
  const [test, what] = __flags[name];
  Object.defineProperty(Assertion.prototype, name, {
    get() { return this._assert(test(this._subject), what); },
  });
}

const __methods = {
  equal(v) {
    return this._deep
      ? this._assert(__deepEqual(this._subject, v), "deeply equal", v)
      : this._assert(this._subject === v, "equal", v);
  },
  eql(v) { return this._assert(__deepEqual(this._subject, v), "deeply equal", v); },
  above(n) { return this._assert(this._subject > n, "be above", n); },
  below(n) { return this._assert(this._subject < n, "be below", n); },
  least(n) { return this._assert(this._subject >= n, "be at least", n); },
  most(n) { return this._assert(this._subject <= n, "be at most", n); },
  within(lo, hi) { return this._assert(this._subject >= lo && this._subject <= hi, "be within", [lo, hi]); },
  a(type) { return this._assert(__typeOf(this._subject) === String(type).toLowerCase(), "be a", type); },
  include(v) {
    const s = this._subject;
    let ok = false;
    if (typeof s === "string") ok = s.includes(v);
    else if (Array.isArray(s)) ok = s.some((x) => (this._deep || typeof v === "object" ? __deepEqual(x, v) : x === v));
    else if (s && typeof s === "object" && v && typeof v === "object") ok = Object.keys(v).every((k) => __deepEqual(s[k], v[k]));
    return this._assert(ok, "include", v);
  },
  property(name, ...value) {
    const s = this._subject;
    const ok = s !== null && s !== undefined && (typeof s === "object" ? name in s : s[name] !== undefined);
    if (value.length === 0) {
      this._assert(ok, "have property", name);
    } else {
      const v = value[0];
      const same = ok && (this._deep ? __deepEqual(s[name], v) : s[name] === v);
      this._assert(same, "have property " + __show(name) + " of", v);
    }
    if (!this._negate) this._subject = ok ? s[name] : undefined;
    return this;
  },
  lengthOf(n) {
    const s = this._subject;
    const len = s === null || s === undefined ? undefined : s.length !== undefined ? s.length : Object.keys(s).length;
    return this._assert(len === n, "have length", n);
  },
  match(re) { return this._assert(re instanceof RegExp && re.test(String(this._subject)), "match", String(re)); },
  string(part) { return this._assert(String(this._subject).includes(part), "contain", part); },
  keys(...names) {
    const list = names.length === 1 && Array.isArray(names[0]) ? names[0] : names;
    const s = this._subject || {};
    return this._assert(list.every((k) => __has(s, k)), "have keys", list);
  },
  oneOf(list) { return this._assert(list.some((x) => __deepEqual(x, this._subject)), "be one of", list); },
  members(list) {
    const s = this._subject;
    const ok = Array.isArray(s) && s.length === list.length && list.every((x) => s.some((y) => __deepEqual(x, y)));
    return this._assert(ok, "have the same members as", list);
  },
  satisfy(fn) { return this._assert(fn(this._subject), "satisfy the given function"); },
};
const __aliases = {
  equal: ["equals", "eq"],
  eql: ["eqls"],
  above: ["gt", "greaterThan"],
  below: ["lt", "lessThan"],
  least: ["gte"],
  most: ["lte"],
  a: ["an"],
  include: ["includes", "contain", "contains"],
  lengthOf: ["length"],
  match: ["matches"],
  keys: ["key"],
};
for (const name of Object.keys(__methods)) {
  for (const alias of [name, ...(__aliases[name] || [])]) {
    Object.defineProperty(Assertion.prototype, alias, { value: __methods[name], writable: true });
  }
}

function expect(subject, message) {
  return new Assertion(subject, message);
}
expect.fail = (message) => {
  throw new AssertionError(message || "expect.fail()");
};
globalThis.expect = expect;

function __test(name, fn) {
  try {
    if (typeof fn === "function") fn();
    __tests.push({ name: String(name), passed: true });
  } catch (e) {
    __tests.push({ name: String(name), passed: false, message: e && e.message ? e.message : String(e) });
  }
}

// ---------------------------------------------------------------- request

const __request = {
  method: __input.request.method,
  url: __input.request.url,
  headers: __input.request.headers.map(([k, v]) => [k, v]),
  body: __input.request.body,
};
const __findHeader = (name) =>
  __request.headers.findIndex(([k]) => k.toLowerCase() === String(name).toLowerCase());

const __pmHeaders = {
  get: (name) => {
    const i = __findHeader(name);
    return i < 0 ? undefined : __request.headers[i][1];
  },
  has: (name) => __findHeader(name) >= 0,
  add: (h) => {
    __request.headers.push([String(h.key), __text(h.value)]);
  },
  upsert: (h) => {
    const i = __findHeader(h.key);
    if (i < 0) __request.headers.push([String(h.key), __text(h.value)]);
    else __request.headers[i][1] = __text(h.value);
  },
  remove: (name) => {
    let i;
    while ((i = __findHeader(name)) >= 0) __request.headers.splice(i, 1);
  },
  toObject: () => Object.fromEntries(__request.headers),
  each: (fn) => __request.headers.forEach(([key, value]) => fn({ key, value })),
};

// ---------------------------------------------------------------- response

const __response = __input.response;
let __parsed;
const __json = () => {
  if (__parsed === undefined) __parsed = JSON.parse(__response.body);
  return __parsed;
};
const __bodyValue = () => {
  try {
    return __json();
  } catch (e) {
    return __response.body;
  }
};
const __responseHeader = (name) => {
  const lower = String(name).toLowerCase();
  const values = __response.headers.filter(([k]) => k.toLowerCase() === lower).map(([, v]) => v);
  return values.length ? values.join(", ") : undefined;
};

function __responseAssertions(negate) {
  const check = (ok, what) => {
    if (Boolean(ok) === negate) {
      throw new AssertionError("expected response " + (negate ? "not " : "") + "to " + what);
    }
  };
  const status = () => __response.status;
  const self = {
    have: {
      status: (s) =>
        check(typeof s === "number" ? status() === s : __response.statusText === s, "have status " + __show(s) + " but got " + status()),
      header: (name, value) => {
        const actual = __responseHeader(name);
        check(value === undefined ? actual !== undefined : actual === value, "have header " + __show(name) + (value === undefined ? "" : " = " + __show(value)));
      },
      body: (part) => check(part === undefined ? __response.body.length > 0 : __response.body.includes(part), "have body " + __show(part)),
      jsonBody: (path) => {
        let ok = true;
        try {
          let v = __json();
          if (path !== undefined) for (const key of String(path).split(".")) v = v == null ? undefined : v[key];
          ok = path === undefined || v !== undefined;
        } catch (e) {
          ok = false;
        }
        check(ok, "have JSON body" + (path === undefined ? "" : " with " + __show(path)));
      },
    },
    be: {},
    get not() {
      return __responseAssertions(!negate);
    },
  };
  const classes = {
    ok: (s) => s >= 200 && s < 300,
    success: (s) => s >= 200 && s < 300,
    info: (s) => s >= 100 && s < 200,
    redirection: (s) => s >= 300 && s < 400,
    clientError: (s) => s >= 400 && s < 500,
    serverError: (s) => s >= 500,
    error: (s) => s >= 400,
    accepted: (s) => s === 202,
    badRequest: (s) => s === 400,
    unauthorized: (s) => s === 401,
    forbidden: (s) => s === 403,
    notFound: (s) => s === 404,
  };
  for (const name of Object.keys(classes)) {
    Object.defineProperty(self.be, name, { get: () => check(classes[name](status()), "be " + name + " but got " + status()) });
  }
  Object.defineProperty(self.be, "json", {
    get: () => {
      let ok = true;
      try { __json(); } catch (e) { ok = false; }
      check(ok, "be JSON");
    },
  });
  self.have.be = self.be;
  self.to = self;
  return self;
}

// ---------------------------------------------------------------- pm

const pm = {
  variables: __store,
  environment: __store,
  collectionVariables: __store,
  globals: __store,
  iterationData: { get: () => undefined, has: () => false, toObject: () => ({}) },
  info: { requestName: __input.stepName, eventName: __input.phase, iteration: 0, iterationCount: 1 },
  test: __test,
  expect,
  request: {
    get method() { return __request.method; },
    set method(m) { __request.method = String(m).toUpperCase(); },
    url: {
      toString: () => __request.url,
      update: (u) => { __request.url = String(u); },
      getHost: () => { const m = /^[a-z]+:\/\/([^/:?#]+)/i.exec(__request.url); return m ? m[1] : ""; },
    },
    headers: __pmHeaders,
    body: {
      get raw() { return __request.body; },
      update: (b) => { __request.body = typeof b === "string" ? b : b && b.raw !== undefined ? b.raw : __text(b); },
      toString: () => __request.body || "",
    },
  },
  sendRequest: __pmSendRequest,
};
if (__response) {
  pm.response = {
    code: __response.status,
    status: __response.statusText,
    responseTime: __response.timeMs,
    responseSize: __response.body.length,
    text: () => __response.body,
    json: __json,
    headers: {
      get: __responseHeader,
      has: (name) => __responseHeader(name) !== undefined,
      toObject: () => Object.fromEntries(__response.headers.map(([k, v]) => [k.toLowerCase(), v])),
    },
    to: __responseAssertions(false),
  };
}
globalThis.pm = pm;

// Postman's pre-`pm` sandbox, still common in older collections.
globalThis.tests = {};
globalThis.postman = {
  setEnvironmentVariable: __store.set,
  getEnvironmentVariable: __store.get,
  clearEnvironmentVariable: __store.unset,
  setGlobalVariable: __store.set,
  getGlobalVariable: __store.get,
  clearGlobalVariable: __store.unset,
};
globalThis.environment = __store.toObject();
globalThis.globals = globalThis.environment;
if (__response) {
  globalThis.responseBody = __response.body;
  globalThis.responseCode = { code: __response.status, name: __response.statusText };
  globalThis.responseTime = __response.timeMs;
  globalThis.responseHeaders = pm.response.headers.toObject();
}

// ---------------------------------------------------------------- bru

globalThis.bru = {
  getVar: __store.get,
  setVar: __store.set,
  hasVar: __store.has,
  deleteVar: __store.unset,
  getEnvVar: __store.get,
  setEnvVar: __store.set,
  getCollectionVar: __store.get,
  getRequestVar: __store.get,
  getProcessEnv: () => undefined,
  interpolate: __store.replaceIn,
  sendRequest: __bruSendRequest,
  // Waits for real, up to the script's time limit, then resolves.
  sleep: (ms) => {
    __sleep(Math.max(0, Number(ms) || 0));
    return Promise.resolve();
  },
};
globalThis.req = {
  getUrl: () => __request.url,
  setUrl: (u) => { __request.url = String(u); },
  getMethod: () => __request.method,
  setMethod: (m) => { __request.method = String(m).toUpperCase(); },
  getHeader: __pmHeaders.get,
  getHeaders: __pmHeaders.toObject,
  setHeader: (k, v) => __pmHeaders.upsert({ key: k, value: v }),
  deleteHeader: __pmHeaders.remove,
  getBody: (options) => {
    if (options && options.raw) return __request.body;
    try { return JSON.parse(__request.body); } catch (e) { return __request.body; }
  },
  setBody: (b) => { __request.body = __text(b); },
  getName: () => __input.stepName,
};
if (__response) {
  globalThis.res = {
    status: __response.status,
    statusText: __response.statusText,
    get body() { return __bodyValue(); },
    headers: pm.response.headers.toObject(),
    responseTime: __response.timeMs,
    getStatus: () => __response.status,
    getStatusText: () => __response.statusText,
    getBody: __bodyValue,
    getHeader: __responseHeader,
    getHeaders: pm.response.headers.toObject,
    getResponseTime: () => __response.timeMs,
  };
}
globalThis.test = __test;

// Bruno `assert` lines become `__bruAssert(value, "op", "argument")`.
function __bruValue(raw) {
  const t = String(raw).trim();
  if (t === "") return undefined;
  if (/^(true|false|null)$/.test(t)) return JSON.parse(t);
  if (/^-?\d+(\.\d+)?$/.test(t)) return Number(t);
  if (/^(['"`]).*\1$/.test(t)) return t.slice(1, -1);
  if (/^[\[{]/.test(t)) {
    try { return JSON.parse(t); } catch (e) {}
  }
  return t;
}
const __bruList = (arg) => (Array.isArray(arg) ? arg : String(arg).split(",").map(__bruValue));

globalThis.__bruAssert = function (actual, op, raw) {
  const arg = __bruValue(raw);
  const e = expect(actual);
  switch (op) {
    case "eq": return e.to.eql(arg);
    case "neq": return e.not.to.eql(arg);
    case "gt": return e.to.be.above(arg);
    case "gte": return e.to.be.at.least(arg);
    case "lt": return e.to.be.below(arg);
    case "lte": return e.to.be.at.most(arg);
    case "in": return e.to.be.oneOf(__bruList(arg));
    case "notIn": return e.not.to.be.oneOf(__bruList(arg));
    case "contains": return e.to.include(arg);
    case "notContains": return e.not.to.include(arg);
    case "length": return e.to.have.lengthOf(arg);
    case "matches": return e.to.match(new RegExp(arg));
    case "notMatches": return e.not.to.match(new RegExp(arg));
    case "startsWith": return e.to.satisfy((v) => String(v).startsWith(arg));
    case "endsWith": return e.to.satisfy((v) => String(v).endsWith(arg));
    case "between": {
      const [lo, hi] = __bruList(arg);
      return e.to.be.within(lo, hi);
    }
    case "isEmpty": return e.to.be.empty;
    case "isNotEmpty": return e.not.to.be.empty;
    case "isNull": return e.to.be.null;
    case "isUndefined": return e.to.be.undefined;
    case "isDefined": return e.not.to.be.undefined;
    case "isTruthy": return e.to.be.ok;
    case "isFalsy": return e.not.to.be.ok;
    case "isJson": return e.to.satisfy((v) => v !== null && typeof v === "object");
    case "isNumber": return e.to.be.a("number");
    case "isString": return e.to.be.a("string");
    case "isBoolean": return e.to.be.a("boolean");
    case "isArray": return e.to.be.an("array");
    default: throw new Error("unknown assert operator " + op);
  }
};

// ---------------------------------------------------------------- run

const __describe = (e) =>
  e && e.message ? (e.name && e.name !== "Error" ? e.name + ": " : "") + e.message : String(e);
let __error = null;
let __settled = false;

// Bruno and Postman run scripts as async functions, so top-level `await`
// works. Do the same: start the script here, let Rust drain the job queue,
// then collect the results with `__finish`.
globalThis.__run = function (source) {
  const AsyncFunction = (async function () {}).constructor;
  let body;
  try {
    body = new AsyncFunction(source);
  } catch (e) {
    __error = __describe(e);
    __settled = true;
    return;
  }
  body().then(
    () => { __settled = true; },
    (e) => { __error = __describe(e); __settled = true; },
  );
};

globalThis.__finish = function () {
  let error = __error;
  if (!__settled && error === null) {
    error = "the script awaited something that never finished";
  }
  for (const name of Object.keys(globalThis.tests)) {
    __tests.push({ name, passed: Boolean(globalThis.tests[name]), message: globalThis.tests[name] ? undefined : "was falsy" });
  }
  return JSON.stringify({
    variables: __changes,
    tests: __tests,
    logs: __logs,
    request: __request,
    error,
  });
};
