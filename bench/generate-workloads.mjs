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

function createDocument(targetBytes) {
  let content = header;
  while (Buffer.byteLength(content) + Buffer.byteLength(block) <= targetBytes) {
    content += block;
  }

  const remaining = targetBytes - Buffer.byteLength(content);
  if (remaining === 1) {
    content += "\n";
  } else if (remaining > 1) {
    content += `${"a".repeat(remaining - 1)}\n`;
  }

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
