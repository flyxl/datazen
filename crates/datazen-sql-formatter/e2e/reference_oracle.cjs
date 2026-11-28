const fs = require("node:fs");
const vm = require("node:vm");
const {createHash} = require("node:crypto");
const packagePath = process.argv[2];
const nodeFormatter = require(packagePath);
const pkg = require(packagePath + "/package.json");
if (pkg.version !== "15.8.2") process.exit(3);
const browser = vm.createContext({});
const bundle = fs.readFileSync(packagePath + "/dist/sql-formatter.min.js");
const expected = process.argv[3];
if (createHash("sha256").update(bundle).digest("hex") !== expected) {
    throw new Error("unrecognized sql-formatter 15.8.2 browser bundle");
}
vm.runInContext(bundle.toString("utf8"), browser);
function format(implementation, sql, options) {
    try { return { output: implementation.format(sql, options) }; }
    catch (_) { return { rejected: true }; }
}
const result = {};
for (const {id, sql, jsOptions} of JSON.parse(fs.readFileSync(0, "utf8"))) {
    const cjs = format(nodeFormatter, sql, jsOptions);
    const web = format(browser.sqlFormatter, sql, jsOptions);
    if (JSON.stringify(cjs) !== JSON.stringify(web)) throw new Error("oracle disagreement: " + id);
    result[id] = cjs;
}
process.stdout.write(JSON.stringify(result));
