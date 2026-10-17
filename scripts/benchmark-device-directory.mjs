// Read-only directory latency diagnostic; prints counts/timings, never filenames.
import { readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { performance } from "node:perf_hooks";

const [adb, serial, path = "/system/bin"] = process.argv.slice(2);
if (!adb || !serial || !path.startsWith("/") || /[\x00-\x1f]/.test(path)) {
  throw new Error("Usage: node scripts/benchmark-device-directory.mjs <adb-path> <serial> [remote-directory]");
}
const quote = value => `'${value.replaceAll("'", "'\\''")}'`;
function run(script) {
  const started = performance.now();
  const result = spawnSync(adb, ["-s", serial, "shell", "-T", "sh", "-c", quote(script)], { timeout: 60000, maxBuffer: 16 * 1024 * 1024, windowsHide: true });
  if (result.error || result.status !== 0) throw result.error ?? new Error(result.stderr.toString());
  return { bytes: result.stdout, ms: Math.round(performance.now() - started) };
}
const source = readFileSync("src-tauri/src/device_files.rs", "utf8");
const raw = source.match(/fn listing_script\(path: &str\)[\s\S]*?r#"([\s\S]*?)"#/)[1];
const script = raw.replace("{}", "__BENCHMARK_REMOTE_PATH__").replaceAll("{{", "{").replaceAll("}}", "}").replace("__BENCHMARK_REMOTE_PATH__", quote(path));
function validate(bytes) {
  const fields = bytes.toString("utf8").split("\0");
  if (fields.shift() !== "STFILES3" || fields.at(-1) !== "") throw new Error("Incomplete listing protocol");
  let entries = 0;
  while (fields.length) {
    const batch = fields.shift();
    if (batch === "DIRS") {
      if (fields.some(name => name && !name.startsWith("./"))) throw new Error("Invalid directory framing");
      return entries;
    }
    const count = Number(batch);
    if (!Number.isInteger(count) || count < 1 || count > 20000) throw new Error("Invalid batch count");
    const names = fields.splice(0, count);
    const rows = fields.shift().trimEnd().split(/\r?\n/);
    if (names.length !== count || names.some(name => !name.startsWith("./")) || rows.length !== count
      || rows.some(row => !/^\d+ -?\d+ [0-7]+ [\da-f]+$/.test(row))) throw new Error(JSON.stringify({ error: "Batch metadata mismatch", count, names: names.length, validNames: names.every(name => name.startsWith("./")), rows: rows.length, numericSample: rows[0].replace(/[^\da-f \r\n-]/gi, "?") }));
    entries += count;
  }
  throw new Error("Missing directory marker");
}
const probe = run("stat -c '%n\\000%s %Y %a %f\\000' -- /system/bin/sh /system/bin/ls");
console.log(JSON.stringify({ probe: "stat-format", nulBytes: [...probe.bytes].filter(byte => byte === 0).length, literalEscapes: probe.bytes.includes(Buffer.from("\\000")) }));
for (let runIndex = 1; runIndex <= 3; ++runIndex) {
  const result = run(script);
  console.log(JSON.stringify({ run: runIndex, path, ms: result.ms, entries: validate(result.bytes), bytes: result.bytes.length }));
}
