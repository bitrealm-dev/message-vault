import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';
import starlightSidebarTopics from 'starlight-sidebar-topics';
import mermaid from 'astro-mermaid';

const limitedBadge = {
  text: 'Limited',
  variant: 'caution',
};

const userGuideItems = [
  { label: 'Home', slug: 'vault/user' },
  {
    label: 'Get started',
    items: [
      'vault/user/get-started/what-is-message-vault',
      'vault/user/get-started/why-you-provide-backups',
      'vault/user/get-started/try-the-vault',
      'vault/user/get-started/your-own-messages',
      'vault/user/get-started/install-the-desktop-app',
    ],
  },
  {
    label: 'Prepare a backup',
    items: [
      'vault/user/prepare-a-backup',
      'vault/user/prepare-a-backup/iphone-ipad',
      'vault/user/prepare-a-backup/iphone-whatsapp',
      'vault/user/prepare-a-backup/android-sms',
      'vault/user/prepare-a-backup/android-whatsapp',
    ],
  },
  'vault/user/import-from-a-backup',
  'vault/user/browse-your-messages',
  {
    label: 'How do I…',
    items: [
      'vault/user/how-to/search',
      'vault/user/how-to/contacts-and-labels',
      'vault/user/how-to/saved-searches',
      'vault/user/how-to/trash',
      'vault/user/how-to/settings',
      'vault/user/how-to/convert-formats',
      'vault/user/how-to/extract-to-files',
      'vault/user/how-to/export-from-the-vault',
      'vault/user/how-to/media-and-privacy',
      { slug: 'vault/user/how-to/rescue-imports', badge: limitedBadge },
      'vault/user/how-to/update',
      'vault/user/how-to/troubleshooting',
    ],
  },
  'vault/user/glossary',
];

const developerItems = [
  'vault/developer',
  'vault/developer/contributing',
  'vault/developer/release',
  'vault/developer/rustdoc-style',
  {
    label: 'Architecture',
    items: [
      'vault/developer/vault-design',
      'vault/developer/message-transfer',
      'vault/developer/architecture/common-message',
    ],
  },
  'vault/developer/docker',
  {
    label: 'CLI tools',
    items: [
      'vault/developer/reference/cli',
      {
        label: 'Supported',
        items: [
          'vault/developer/reference/cli/imessage-ir-exporter',
          'vault/developer/reference/cli/sms-backup-restore-exporter',
          'vault/developer/reference/cli/whatsapp-exporter',
        ],
      },
      {
        label: 'Rescue / experimental',
        items: [
          {
            slug: 'vault/developer/reference/cli/go-sms-pro-exporter',
            badge: limitedBadge,
          },
          {
            slug: 'vault/developer/reference/cli/imazing-exporter',
            badge: limitedBadge,
          },
          {
            slug: 'vault/developer/reference/cli/openextract-exporter',
            badge: limitedBadge,
          },
          {
            slug: 'vault/developer/reference/cli/sms-backup-plus-exporter',
            badge: limitedBadge,
          },
        ],
      },
      {
        label: 'Vault JSONL',
        items: [
          'vault/developer/reference/cli/message-reexporter',
          'vault/developer/reference/cli/vault-push',
          'vault/developer/reference/cli/vault-pull',
        ],
      },
    ],
  },
  'vault/developer/reference/api',
  {
    label: 'HTTP API reference',
    link: '/vault/developer/rustdoc/http/',
    attrs: { target: '_self' },
  },
  {
    label: 'Rust crate docs',
    link: '/vault/developer/rustdoc/',
    attrs: { target: '_self' },
  },
  {
    label: 'Formats',
    items: [
      'vault/developer/formats',
      'vault/developer/formats/mail-archive',
      'vault/developer/formats/sms-backup-restore-xml',
      'vault/developer/formats/convert',
      {
        label: 'SMS Backup & Restore',
        items: [
          'vault/developer/formats/sms-backup-restore/input',
          'vault/developer/formats/sms-backup-restore/mapping',
        ],
      },
      {
        label: 'SMS Backup+',
        items: [
          'vault/developer/formats/sms-backup-plus/format',
          'vault/developer/formats/sms-backup-plus/mapping',
        ],
      },
      {
        label: 'GO SMS Pro',
        items: ['vault/developer/formats/go-sms-pro/mapping'],
      },
      {
        label: 'iMazing',
        items: [
          'vault/developer/formats/imazing/input',
          'vault/developer/formats/imazing/design',
        ],
      },
    ],
  },
  {
    label: 'Instance internals',
    collapsed: true,
    items: [
      'vault/developer/reference/config-and-accounts',
      'vault/developer/reference/database',
      'vault/developer/reference/export-structure',
      'vault/developer/reference/csv-columns',
      'vault/developer/reference/server-cli',
    ],
  },
];

export default defineConfig({
  site: 'https://bitrealm.io',
  redirects: {
    '/vault/developer/docker-compose/': '/vault/developer/docker/',
  },
  integrations: [
    mermaid({
      autoTheme: true,
      enableLog: false,
    }),
    starlight({
      title: 'Message Vault',
      description:
        'Extract messages from phone backups, import them into a local vault, and browse them in a website you control.',
      editLink: {
        baseUrl:
          'https://github.com/bitrealm-io/message-vault/edit/main/docs/',
      },
      social: [
        {
          icon: 'github',
          label: 'GitHub',
          href: 'https://github.com/bitrealm-io/message-vault',
        },
      ],
      customCss: ['./src/styles/custom.css'],
      plugins: [
        starlightSidebarTopics(
          [
            {
              label: 'User Guide',
              link: '/vault/user/',
              icon: 'open-book',
              items: userGuideItems,
            },
            {
              label: 'Developer',
              id: 'developer',
              link: '/vault/developer/',
              icon: 'laptop',
              items: developerItems,
            },
          ],
          {
            topics: {
              developer: [
                '/vault/developer/rustdoc',
                '/vault/developer/rustdoc/**',
              ],
            },
          },
        ),
      ],
    }),
  ],
});
