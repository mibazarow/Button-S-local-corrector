import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";

const manifest = "src-tauri/Cargo.toml";
const rustc = execFileSync("rustc", ["-vV"], { encoding: "utf8" });
const host = rustc.match(/^host:\s*(\S+)$/m)?.[1];

if (!host) {
  throw new Error("cannot detect Rust host triple");
}

execFileSync(
  "cargo",
  ["build", "--manifest-path", manifest, "-p", "button-s-llm-worker", "--release"],
  { stdio: "inherit" },
);

const source = join("src-tauri", "target", "release", "button-s-llm");
const destination = join("src-tauri", "binaries", `button-s-llm-${host}`);
mkdirSync(dirname(destination), { recursive: true });
copyFileSync(source, destination);
console.log(`llm worker -> ${destination}`);
