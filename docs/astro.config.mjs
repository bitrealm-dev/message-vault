import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';
import starlightSidebarTopics from 'starlight-sidebar-topics';

const limitedBadge = {
  text: 'Limited',
  variant: 'caution',
};

const userGuideItems = [
  { label: 'Home', slug: '' },
  {
    label: 'Get started',
    items: [
      'get-started/what-is-message-vault',
      'get-started/why-you-provide-backups',
      'get-started/try-the-vault',
      'get-started/your-own-messages',
      'get-started/install-the-desktop-app',
    ],
  },
  {
    label: 'Prepare a backup',
    items: [
      'prepare-a-backup',
      'prepare-a-backup/iphone-ipad',
      'prepare-a-backup/iphone-whatsapp',
      'prepare-a-backup/android-sms',
      'prepare-a-backup/android-whatsapp',
    ],
  },
  'import-from-a-backup',
  'browse-your-messages',
  {
    label: 'How do I…',
    items: [
      'how-to/search',
      'how-to/contacts-and-labels',
      'how-to/saved-searches',
      'how-to/trash',
      'how-to/settings',
      'how-to/convert-formats',
      'how-to/extract-to-files',
      'how-to/export-from-the-vault',
      'how-to/media-and-privacy',
      { slug: 'how-to/rescue-imports', badge: limitedBadge },
      'how-to/update',
      'how-to/troubleshooting',
    ],
  },
  'glossary',
];

const developerItems = [
  'developer/run-from-source',
  'developer/docker-compose',
  {
    label: 'CLI tools',
    items: [
      'reference/cli',
      'reference/cli/imessage-ir-exporter',
      'reference/cli/sms-backup-restore-exporter',
      'reference/cli/whatsapp-exporter',
      'reference/cli/message-reexporter',
      'reference/cli/vault-push',
      'reference/cli/vault-pull',
      'reference/cli/go-sms-pro-exporter',
      'reference/cli/imazing-exporter',
      'reference/cli/openextract-exporter',
      'reference/cli/sms-backup-plus-exporter',
    ],
  },
  'reference/api',
  {
    label: 'Formats',
    items: [
      'formats',
      'formats/mail-archive',
      'formats/sms-backup-restore-xml',
      'formats/convert',
      {
        label: 'SMS Backup & Restore',
        items: [
          'formats/sms-backup-restore/input',
          'formats/sms-backup-restore/mapping',
        ],
      },
      {
        label: 'SMS Backup+',
        items: [
          'formats/sms-backup-plus/format',
          'formats/sms-backup-plus/mapping',
        ],
      },
      {
        label: 'GO SMS Pro',
        items: ['formats/go-sms-pro/mapping'],
      },
      {
        label: 'iMazing',
        items: ['formats/imazing/input', 'formats/imazing/design'],
      },
    ],
  },
  {
    label: 'Instance internals',
    collapsed: true,
    items: [
      'reference/config-and-accounts',
      'reference/database',
      'reference/export-structure',
      'reference/csv-columns',
      'reference/server-cli',
    ],
  },
];

export default defineConfig({
  site: 'https://bitrealm.dev',
  integrations: [
    starlight({
      title: 'Message Vault',
      description:
        'Extract messages from phone backups, import them into a local vault, and browse them in a website you control.',
      editLink: {
        baseUrl:
          'https://github.com/bitrealm-dev/message-vault/edit/main/docs/',
      },
      social: [
        {
          icon: 'github',
          label: 'GitHub',
          href: 'https://github.com/bitrealm-dev/message-vault',
        },
      ],
      customCss: ['./src/styles/custom.css'],
      plugins: [
        starlightSidebarTopics([
          {
            label: 'User Guide',
            link: '/',
            icon: 'open-book',
            items: userGuideItems,
          },
          {
            label: 'Developer',
            link: '/developer/run-from-source/',
            icon: 'laptop',
            items: developerItems,
          },
        ]),
      ],
    }),
  ],
});
