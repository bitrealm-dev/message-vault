import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const docsDirectory = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  '..',
);
const repositoryDirectory = path.resolve(docsDirectory, '..');
const outputDirectory = path.join(
  docsDirectory,
  'src/content/docs/reference/cli',
);

const pages = [
  {
    slug: 'imessage-ir-exporter',
    title: 'iPhone backup (imessage-ir-exporter)',
    description:
      'Command-line options for exporting Apple Messages from an iPhone backup or chat database.',
    source: 'crates/exporters/imessage-ir-exporter/docs/MANPAGE.md',
  },
  {
    slug: 'sms-backup-restore-exporter',
    title: 'SMS Backup & Restore',
    description:
      'Command-line options for converting an SMS Backup & Restore XML file.',
    source: 'crates/exporters/sms-backup-restore-exporter/docs/MANPAGE.md',
  },
  {
    slug: 'whatsapp-exporter',
    title: 'WhatsApp',
    description:
      'Command-line options for extracting and converting Apple or Android WhatsApp backups.',
    source: 'crates/exporters/whatsapp-exporter/docs/MANPAGE.md',
  },
  {
    slug: 'message-reexporter',
    title: 'Convert an existing export',
    description:
      'Command-line options for converting a Message Vault output directory to another format.',
    source: 'crates/libs/reexport/docs/MESSAGE_REEXPORTER.md',
  },
  {
    slug: 'vault-push',
    title: 'Push to Message Vault',
    description:
      'Command-line options for importing a JSONL export folder into Message Vault.',
    source: 'crates/cli/vault-push/docs/MANPAGE.md',
  },
  {
    slug: 'vault-pull',
    title: 'Pull from Message Vault',
    description:
      'Command-line options for downloading messages from Message Vault into a local JSONL folder.',
    source: 'crates/cli/vault-pull/docs/MANPAGE.md',
  },
  {
    slug: 'go-sms-pro-exporter',
    title: 'GO SMS Pro',
    description:
      'Command-line options for rescuing messages from a GO SMS Pro XML export.',
    source: 'crates/exporters/go-sms-pro-exporter/docs/MANPAGE.md',
  },
  {
    slug: 'imazing-exporter',
    title: 'iMazing',
    description:
      'Command-line options for rescuing messages from an iMazing CSV export.',
    source: 'crates/exporters/imazing-exporter/docs/MANPAGE.md',
  },
  {
    slug: 'openextract-exporter',
    title: 'OpenExtract',
    description:
      'Command-line options for rescuing messages from an OpenExtract export.',
    source: 'crates/exporters/openextract-exporter/docs/MANPAGE.md',
  },
  {
    slug: 'sms-backup-plus-exporter',
    title: 'SMS Backup+',
    description:
      'Command-line options for rescuing messages from an SMS Backup+ mail archive.',
    source: 'crates/exporters/sms-backup-plus-exporter/docs/MANPAGE.md',
  },
];

function repositoryUrl(sourcePath, target) {
  const [linkPath, fragment] = target.split('#', 2);
  const resolvedPath = path.posix.normalize(
    path.posix.join(path.posix.dirname(sourcePath), linkPath),
  );
  const suffix = fragment ? `#${fragment}` : '';
  return `https://github.com/bitrealm-dev/message-vault/blob/main/${resolvedPath}${suffix}`;
}

function prepareBody(markdown, sourcePath) {
  return markdown
    .replace(/^(#{1,5})(?=\s)/gm, '#$1')
    .replace(
      /\]\((?!https?:\/\/|mailto:|#|\/)([^)\s]+)\)/g,
      (_match, target) => `](${repositoryUrl(sourcePath, target)})`,
    )
    .trim();
}

await mkdir(outputDirectory, { recursive: true });

for (const page of pages) {
  const sourceFile = path.join(repositoryDirectory, page.source);
  const body = prepareBody(await readFile(sourceFile, 'utf8'), page.source);
  const editUrl =
    `https://github.com/bitrealm-dev/message-vault/edit/main/${page.source}`;
  const frontmatter = [
    '---',
    `title: ${JSON.stringify(page.title)}`,
    `description: ${JSON.stringify(page.description)}`,
    `editUrl: ${JSON.stringify(editUrl)}`,
    'tableOfContents:',
    '  minHeadingLevel: 2',
    '  maxHeadingLevel: 4',
    '---',
  ].join('\n');

  await writeFile(
    path.join(outputDirectory, `${page.slug}.md`),
    `${frontmatter}\n\n${body}\n`,
  );
}

console.log(`Updated ${pages.length} command-line reference pages.`);
