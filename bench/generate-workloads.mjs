import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const benchDir = dirname(fileURLToPath(import.meta.url));
const workloadsDir = join(benchDir, "workloads");

const workloads = [
  { name: "tiny.md", bytes: 4 * 1024 },
  { name: "medium.md", bytes: 1024 * 1024 },
  { name: "large.md", bytes: 5 * 1024 * 1024 }
];

const header = "# Reproducible benchmark workload\n\n";
const block = [
  "This paragraph contains stable ASCII text and 中文内容 123.",
  "",
  "> quoted text",
  "",
  "Inline `code` remains non-empty.",
  "",
  "```text",
  "sample 123",
  "```",
  ""
].join("\n");
const headerBytes = Buffer.byteLength(header);
const blockBytes = Buffer.byteLength(block);

function createDocument(targetBytes) {
  const chunks = [header];
  let currentBytes = headerBytes;

  while (currentBytes + blockBytes <= targetBytes) {
    chunks.push(block);
    currentBytes += blockBytes;
  }

  const remaining = targetBytes - currentBytes;
  if (remaining === 1) {
    chunks.push("\n");
  } else if (remaining > 1) {
    chunks.push(`${"a".repeat(remaining - 1)}\n`);
  }

  const content = chunks.join("");
  if (Buffer.byteLength(content) !== targetBytes) {
    throw new Error(`failed to generate exactly ${targetBytes} bytes`);
  }
  return content;
}

mkdirSync(workloadsDir, { recursive: true });

const generated = workloads.map(({ name, bytes }) => {
  const path = join(workloadsDir, name);
  const content = createDocument(bytes);
  writeFileSync(path, content);
  return { name, bytes, path };
});

console.log(JSON.stringify({ generated }, null, 2));
