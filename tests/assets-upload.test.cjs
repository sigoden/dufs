const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");
const vm = require("node:vm");

const source = fs.readFileSync(path.join(__dirname, "../assets/index.js"), "utf8");
const file = (name, size = 42) => ({ name, size, mtime: 1234567890000, path_type: "File" });

function setup(paths = [], permissions = {}, search = "") {
  const nodes = new Map();
  function element(id, parent) {
    const classes = new Set(["hidden"]);
    if (/^upload\d+$/.test(id)) classes.add("uploader");
    let html = "";
    const node = {
      id,
      parent,
      get innerHTML() { return html; },
      set innerHTML(value) {
        for (const child of [...nodes.values()]) if (child.parent === node) child.remove();
        html = value;
      },
      classList: {
        add: value => classes.add(value),
        remove: value => classes.delete(value),
        contains: value => classes.has(value),
      },
      addEventListener() {},
      remove() {
        for (const child of [...nodes.values()]) if (child.parent === node) child.remove();
        nodes.delete(id);
      },
      cloneNode() { return { ...node }; },
      parentNode: { replaceChild(replacement) { nodes.set(id, replacement); } },
      insertAdjacentHTML(_position, value) {
        html += value;
        for (const match of value.matchAll(/id="([^"]+)"/g)) element(match[1], node);
      },
      querySelector(selector) {
        return [...nodes.values()].find(value => value.parent === node && value.classList.contains(selector.slice(1)));
      },
    };
    nodes.set(id, node);
    return node;
  }
  const table = element("table"), body = element("body"), uploads = element("uploads"), empty = element("empty");
  const initial = { kind: "Index", paths, dir_exists: true, allow_upload: true, allow_delete: true, ...permissions };
  let response;
  const requests = [];
  const context = vm.createContext({
    URLSearchParams, URL, console,
    window: { location: { search }, addEventListener() {} },
    location: { href: `http://localhost/prefix/sub/${search}` },
    document: {
      getElementById: id => nodes.get(id),
      querySelectorAll: selector => [...nodes.values()].filter(value => value.classList.contains(selector.slice(1))),
    },
    fetch: async (url, options) => {
      requests.push({ url: String(url), options });
      return typeof response === "function" ? response() : response;
    },
    table, body, uploads, empty, initial,
  });
  const run = code => vm.runInContext(code, context);
  run(source);
  run(`
    DATA = initial; DIR_EMPTY_NOTE = "Empty folder";
    $pathsTable = table; $pathsTableBody = body; $uploadersTable = uploads; $emptyFolder = empty;
    Uploader.runQueue = () => {};
    var tokenRoots = [];
    setupDownloadWithToken = root => tokenRoots.push(root.id);
    renderPathsTableBody();
  `);
  const reply = values => { response = { status: 200, json: async () => ({ ...initial, paths: values }) }; };
  reply(paths);
  return {
    nodes, table, body, uploads, empty, requests, run, reply,
    get paths() { return run("DATA.paths"); },
    respond(value) { response = value; },
    upload(name) {
      context.uploadFile = { name, size: 42 };
      run("var current = new Uploader(uploadFile, []); current.upload(); Uploader.runnings = 1;");
    },
  };
}

test("successful uploads show server metadata and actions without reloading", async () => {
  const state = setup();
  state.upload("new.txt");
  // The last existing file may have been deleted while this upload was running.
  state.empty.classList.remove("hidden");
  state.reply([file("new.txt", 99)]);
  await state.run("current.complete()");
  assert.equal(state.paths[0].size, 99);
  assert.equal(state.paths[0].mtime, 1234567890000);
  assert.equal(state.table.classList.contains("hidden"), false);
  assert.equal(state.empty.classList.contains("hidden"), true);
  assert.equal(state.nodes.has("upload0"), false);
  assert.equal(state.uploads.classList.contains("hidden"), true);
  for (const title of ["Edit file", "Move & Rename", "Delete", "Download file"])
    assert.ok(state.body.innerHTML.includes(`title="${title}"`));
});

test("the server decides whether differently cased names are one file or two", async () => {
  for (const names of [["existing.txt"], ["EXISTING.txt", "existing.txt"]]) {
    const state = setup([file("existing.txt")]);
    state.upload("EXISTING.txt");
    state.reply(names.map(name => file(name)));
    await state.run("current.complete()");
    assert.deepEqual(state.paths.map(item => item.name), names);
    assert.equal(state.nodes.has(`addPath${names.length}`), false);
  }
});

for (const allow_upload of [false, true]) for (const allow_delete of [false, true]) {
  test(`refreshed actions respect upload=${allow_upload}, delete=${allow_delete}`, async () => {
    const state = setup([], { allow_upload, allow_delete });
    state.upload("permissions.txt");
    state.reply([file("permissions.txt")]);
    await state.run("current.complete()");
    assert.equal(state.body.innerHTML.includes('title="Delete"'), allow_delete);
    for (const title of ["Edit file", "Move & Rename"])
      assert.equal(state.body.innerHTML.includes(`title="${title}"`), allow_upload && allow_delete);
    assert.equal(state.body.innerHTML.includes('title="View file"'), !(allow_upload && allow_delete));
  });
}

test("refresh preserves search and sorting parameters, server ordering and URL encoding", async () => {
  const state = setup([], {}, "?q=nested&sort=name&order=desc");
  state.upload("new.txt");
  state.reply([file("z.txt"), file("nested dir/a #&中.txt")]);
  await state.run("current.complete()");
  assert.equal(state.requests[0].url, "http://localhost/prefix/sub/?q=nested&sort=name&order=desc&json=");
  assert.equal(state.requests[0].options.cache, "no-store");
  assert.equal(state.paths[0].name, "z.txt");
  assert.ok(state.body.innerHTML.includes("/prefix/sub/nested%20dir/a%20%23%26%E4%B8%AD.txt?edit"));
  assert.match(state.body.innerHTML, /deletePath\(1\)/);
});

test("failed uploads retain retry controls without refreshing the listing", () => {
  const state = setup();
  state.upload("failed.txt");
  state.run('current.fail("403 Forbidden")');
  assert.equal(state.requests.length, 0);
  assert.equal(state.nodes.has("upload0"), true);
  assert.equal(state.nodes.has("addPath0"), false);
  assert.match(state.nodes.get("uploadStatus0").innerHTML, /retry0/);
  assert.equal(state.run("failUploaders.has(0)"), true);
});

for (const [name, response] of [
  ["403", { status: 403 }],
  ["invalid JSON", { status: 200, json: async () => { throw new SyntaxError("Invalid JSON"); } }],
  ["invalid paths", { status: 200, json: async () => ({ kind: "Index", paths: [{}] }) }],
  ["network failure", () => { throw new Error("Connection lost"); }],
]) {
  test(`a ${name} listing response preserves successful uploads and existing rows`, async () => {
    const state = setup([file("existing.txt")]);
    state.upload("new.txt");
    state.respond(response);
    await state.run("current.complete()");
    assert.equal(state.paths.length, 1);
    assert.equal(state.paths[0].name, "existing.txt");
    assert.equal(state.nodes.has("addPath0"), true);
    assert.equal(state.nodes.get("uploadStatus0").innerHTML, "✓");
    assert.equal(state.run("failUploaders.has(0)"), false);
    assert.equal(state.run("Uploader.runnings"), 0);
  });
}

test("an older listing response cannot overwrite a newer refresh", async () => {
  const state = setup();
  let release;
  state.respond(() => new Promise(resolve => { release = resolve; }));
  const older = state.run("refreshPaths()");
  state.reply([file("new.txt")]);
  await state.run("refreshPaths()");
  release({ status: 200, json: async () => ({ kind: "Index", paths: [file("old.txt")] }) });
  await older;
  assert.equal(state.paths[0].name, "new.txt");
});

test("refresh preserves pending upload rows and scopes download token setup", async () => {
  const state = setup([], { user: "test-user" });
  state.upload("first.txt");
  state.run("var first = current;");
  state.upload("second.txt");
  state.reply([file("first.txt")]);
  await state.run("first.complete()");
  assert.equal(state.nodes.has("upload0"), false);
  assert.equal(state.nodes.has("upload1"), true);
  assert.equal(state.uploads.classList.contains("hidden"), false);
  assert.equal(state.run("tokenRoots.join(',')"), "body");
});

test("a pending deletion removes its original file after the list is reordered", async () => {
  const state = setup([file("a.txt"), file("b.txt"), file("c.txt")]);
  state.run("var finishDelete; doDeletePath = async (name, url, callback) => { finishDelete = callback; };");
  await state.run("deletePath(1)");
  state.reply([file("c.txt"), file("a.txt"), file("b.txt")]);
  await state.run("refreshPaths()");
  state.respond({ status: 403 });
  state.run("finishDelete()");
  assert.deepEqual(state.paths.map(item => item?.name ?? null), ["c.txt", "a.txt", null]);
  assert.equal(state.nodes.has("addPath0"), true);
  assert.equal(state.nodes.has("addPath1"), true);
  assert.equal(state.nodes.has("addPath2"), false);
  assert.equal(state.requests.length, 2);
});
