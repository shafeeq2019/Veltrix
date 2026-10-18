// Builds the native messaging host and places it where Tauri's
// `bundle.externalBin` expects it: src-tauri/binaries/odm-native-host-<triple>[.exe]
//
//   node scripts/build-sidecar.mjs                    # host triple, release
//   node scripts/build-sidecar.mjs --debug            # debug build
//   node scripts/build-sidecar.mjs --target aarch64-apple-darwin
//   node scripts/build-sidecar.mjs --target universal-apple-darwin   (macOS, uses lipo)
import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const args = process.argv.slice(2);
const debug = args.includes("--debug");
const ti = args.indexOf("--target");
const hostTriple = /host: (\S+)/.exec(execFileSync("rustc", ["-vV"]).toString())[1];
const target = ti >= 0 ? args[ti + 1] : hostTriple;
const profile = debug ? "debug" : "release";
const outDir = join(root, "src-tauri", "binaries");
mkdirSync(outDir, { recursive: true });

function build(triple) {
  const cargoArgs = ["build", "-p", "odm-native-host", "--target", triple];
  if (!debug) cargoArgs.push("--release");
  execFileSync("cargo", cargoArgs, { cwd: root, stdio: "inherit" });
  const exe = triple.includes("windows") ? ".exe" : "";
  return join(root, "target", triple, profile, `odm-native-host${exe}`);
}

if (target === "universal-apple-darwin") {
  const parts = ["aarch64-apple-darwin", "x86_64-apple-darwin"].map(build);
  const out = join(outDir, "odm-native-host-universal-apple-darwin");
  execFileSync("lipo", ["-create", "-output", out, ...parts], { stdio: "inherit" });
  console.log(`sidecar → ${out}`);
} else {
  const built = build(target);
  const exe = target.includes("windows") ? ".exe" : "";
  const out = join(outDir, `odm-native-host-${target}${exe}`);
  copyFileSync(built, out);
  console.log(`sidecar → ${out}`);
}
