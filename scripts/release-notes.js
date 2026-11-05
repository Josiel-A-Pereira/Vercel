const { readFileSync } = require('node:fs');

const CHANGE_GROUPS = new Set([
  'new features', 'features', 'bug fixes', 'improvements', 'documentation', 'breaking changes',
]);

function validVersion(version) {
  if (typeof version !== 'string') return false;
  const match = version.match(/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$/);
  return Boolean(match && match[0] === version && !match[4]?.split('.').some((part) => /^0\d+$/.test(part)));
}

function findCodeEnd(lines, start, offset, delimiter) {
  for (let index = start; index < lines.length; index += 1) {
    if (index > start && !lines[index].trim()) return undefined;
    const runs = /`+/g;
    runs.lastIndex = index === start ? offset : 0;
    let match;
    while ((match = runs.exec(lines[index]))) {
      if (match[0] === delimiter) return { line: index, offset: runs.lastIndex };
    }
  }
  return undefined;
}

function visibleLines(lines) {
  let fence;
  let codeEnd;
  let comment = false;
  const visible = lines.map((line, index) => {
    const marker = line.match(/^ {0,3}(`{3,}|~{3,})(.*)$/);
    if (fence) {
      if (marker && marker[1][0] === fence[0] && marker[1].length >= fence.length && !marker[2].trim()) {
        fence = undefined;
      }
      return '';
    }
    if (!comment && !codeEnd && /^(?: {4}| {0,3}\t)/.test(line)) return '';
    if (!comment && !codeEnd && marker && (marker[1][0] === '~' || !marker[2].includes('`'))) {
      fence = marker[1];
      return '';
    }
    let text = codeEnd ? '`' : '';
    let offset = 0;
    while (offset < line.length) {
      if (codeEnd) {
        if (index < codeEnd.line) {
          text += line.slice(offset);
          break;
        }
        text += line.slice(offset, codeEnd.offset);
        offset = codeEnd.offset;
        codeEnd = undefined;
        continue;
      }
      const boundary = line.indexOf(comment ? '-->' : '<!--', offset);
      const code = comment ? -1 : line.indexOf('`', offset);
      if (code >= 0 && (boundary < 0 || code < boundary)) {
        const delimiter = line.slice(code).match(/^`+/)[0];
        const after = code + delimiter.length;
        text += line.slice(offset, after);
        offset = after;
        codeEnd = findCodeEnd(lines, index, after, delimiter);
        continue;
      }
      if (boundary < 0) {
        if (!comment) text += line.slice(offset);
        break;
      }
      if (!comment) text += line.slice(offset, boundary);
      offset = boundary + (comment ? 3 : 4);
      comment = !comment;
    }
    return text;
  });
  if (fence) throw new Error('Changelog contains an unclosed code fence');
  if (comment) throw new Error('Changelog contains an unclosed HTML comment');
  return visible;
}

function parseHeading(line, index) {
  const match = line.match(/^ {0,3}(#{1,6})(?:[ \t]+(.*)|[ \t]*)$/);
  if (!match) return undefined;
  return { level: match[1].length, title: (match[2] || '').replace(/[ \t]+#+[ \t]*$/, '').trim(), index };
}

function headingVersion(title) {
  const match = title.match(/^(?:\[([^\]]+)\](?:\([^)]+\))?|([^\s]+))(?:[ \t]+\([^)]+\))?$/);
  const label = match?.[1] || match?.[2];
  if (label?.toLowerCase() === 'unreleased') return undefined;
  if (validVersion(label)) return label;
  if (/^(?:\[)?(?:v?\d|unreleased\b)/i.test(title)) {
    throw new Error(`Malformed release heading: ${title}`);
  }
  return undefined;
}

function hasChanges(lines) {
  const placeholders = new Set(['tbd', 'todo', 'none', 'n/a', 'no changes', 'no changes yet', '...', '…']);
  return lines.some((line) => {
    if (/^ {0,3}#{1,6}(?:[ \t]|$)/.test(line)) return false;
    const text = line.trim().replace(/^(?:[-*+]|\d+[.)])(?:[ \t]+|$)/, '')
      .replace(/[*_`~]/g, '').trim().toLowerCase();
    return text && !/^[.\-–—…]+$/.test(text) && !placeholders.has(text.replace(/[.!]$/, ''));
  });
}

function validateContributors(lines) {
  const entries = lines.filter((line) => line.trim());
  if (!entries.length) throw new Error('Contributors must contain at least one - @handle entry');
  const seen = new Set();
  for (const entry of entries) {
    const handle = entry.trim().match(/^-[ \t]+@([a-z\d-]+)$/i)?.[1];
    if (!handle || !/^[a-z\d](?:[a-z\d]|-(?=[a-z\d])){0,38}$/i.test(handle)) {
      throw new Error('Contributors must contain only - @handle entries with valid non-bot GitHub handles');
    }
    const key = handle.toLowerCase();
    if (seen.has(key)) throw new Error(`Duplicate contributor: @${handle}`);
    seen.add(key);
  }
}

function releaseNotes(markdown, version) {
  if (!validVersion(version)) throw new Error('Expected a valid release version, such as 1.2.3');
  const lines = markdown.split(/\r?\n/);
  const visible = visibleLines(lines);
  const headings = visible.map(parseHeading).filter(Boolean);
  const releases = headings.filter((heading) => heading.level === 2)
    .map((heading) => ({ ...heading, version: headingVersion(heading.title) }));
  const matches = releases.filter((heading) => heading.version === version);
  if (!matches.length) throw new Error(`Release ${version} was not found in the changelog`);
  if (matches.length !== 1) throw new Error(`Duplicate release heading for ${version}`);
  const release = matches[0];
  if (releases.find((heading) => heading.version) !== release) {
    throw new Error(`Release ${version} must be the latest numbered release`);
  }
  const end = headings.find((heading) => heading.index > release.index && heading.level <= 2)?.index ?? lines.length;
  const sections = headings.filter((heading) => heading.level === 3 && heading.index > release.index && heading.index < end)
    .map((heading) => ({
      ...heading,
      name: heading.title.replace(/^⚠\uFE0F?[ \t]*/, '').toLowerCase(),
      lines: visible.slice(heading.index + 1,
        headings.find((next) => next.index > heading.index && next.level <= 3)?.index ?? end),
    }));
  const groups = sections.filter((section) => CHANGE_GROUPS.has(section.name));
  if (!groups.length) throw new Error('Release notes must contain at least one nonempty change group');
  for (const group of groups) {
    if (!hasChanges(group.lines)) throw new Error(`Change group ${group.title} is empty or contains only placeholders`);
  }
  const contributors = sections.filter((section) => section.name === 'contributors');
  if (contributors.length !== 1) throw new Error('Release notes must contain exactly one Contributors section');
  validateContributors(contributors[0].lines);
  let start = release.index + 1;
  let stop = end;
  while (start < stop && !lines[start].trim()) start += 1;
  while (stop > start && !lines[stop - 1].trim()) stop -= 1;
  const newline = markdown.includes('\r\n') ? '\r\n' : '\n';
  return `${lines.slice(start, stop).join(newline)}${newline}`;
}

if (require.main === module) {
  try {
    const [version, path = 'CHANGELOG.md', ...extra] = process.argv.slice(2);
    if (!version || extra.length) throw new Error('Usage: node scripts/release-notes.js VERSION [PATH|-]');
    const notes = releaseNotes(readFileSync(path === '-' ? 0 : path, 'utf8'), version);
    process.stdout.write(notes);
  } catch (error) {
    process.stderr.write(`${error.message}\n`);
    process.exitCode = 1;
  }
}

module.exports = { releaseNotes };
