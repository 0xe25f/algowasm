import { readFile, readdir } from 'node:fs/promises';
import path from 'node:path';

const ignoredDirs = new Set([
  '.git',
  'node_modules',
  'target',
  'dist',
  'pkg',
  'playwright-report',
  'test-results'
]);

const checkedExtensions = new Set(['.md']);
const ignoredFiles = new Set([
  'AGENTS.md',
  'AlgoWASM_IMPLEMENTATION_PROMPT_AND_PLAN.md',
  'LICENSE'
]);
const banned = [
  ['initialize', 'initialise'],
  ['initialization', 'initialisation'],
  ['optimize', 'optimise'],
  ['optimization', 'optimisation'],
  ['behavior', 'behaviour'],
  ['color', 'colour'],
  ['serialize', 'serialise'],
  ['serialization', 'serialisation']
];

async function* walk(dir) {
  const entries = await readdir(dir, { withFileTypes: true });

  for (const entry of entries) {
    if (ignoredDirs.has(entry.name)) {
      continue;
    }

    const fullPath = path.join(dir, entry.name);

    if (entry.isDirectory()) {
      yield* walk(fullPath);
      continue;
    }

    if (ignoredFiles.has(entry.name)) {
      continue;
    }

    if (checkedExtensions.has(path.extname(entry.name))) {
      yield fullPath;
    }
  }
}

const failures = [];

for await (const filePath of walk(process.cwd())) {
  const text = await readFile(filePath, 'utf8');
  const lower = text.toLowerCase();

  for (const [bad, good] of banned) {
    if (lower.includes(bad)) {
      failures.push(`${filePath}: use "${good}" rather than "${bad}"`);
    }
  }
}

if (failures.length > 0) {
  console.error(failures.join('\n'));
  process.exit(1);
}

console.log('British English check passed.');
