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

const checkedExtensions = new Set([
  '.c',
  '.cpp',
  '.css',
  '.html',
  '.ini',
  '.js',
  '.json',
  '.md',
  '.mjs',
  '.rs',
  '.toml',
  '.ts',
  '.tsx',
  '.yml'
]);

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

    if (checkedExtensions.has(path.extname(entry.name))) {
      yield fullPath;
    }
  }
}

const failures = [];

for await (const filePath of walk(process.cwd())) {
  const text = await readFile(filePath, 'utf8');
  const lines = text.split('\n');

  lines.forEach((line, index) => {
    if (line.includes('\t')) {
      failures.push(`${filePath}:${index + 1}: contains a tab`);
    }

    const leading = line.match(/^ */)?.[0].length ?? 0;
    // Continuation lines of a block comment start with an asterisk that sits
    // one space in, under the comment's opening slash. That space aligns the
    // comment rather than indenting code, so those lines are not counted.
    const commentContinuation = line.trimStart().startsWith('*');

    if (leading % 2 !== 0 && !commentContinuation) {
      failures.push(`${filePath}:${index + 1}: indentation is not a multiple of 2 spaces`);
    }
  });
}

if (failures.length > 0) {
  console.error(failures.join('\n'));
  process.exit(1);
}

console.log('Indentation check passed.');
