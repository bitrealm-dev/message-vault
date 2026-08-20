import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';
import starlightSidebarTopics from 'starlight-sidebar-topics';

const limitedBadge = {
  text: 'Limited',
  variant: 'caution',
};

const userGuideItems = [
  { label: 'Home', slug: 'user' },
  {
    label: 'Get started',
    items: [
      'user/get-started/what-is-message-vault',
      'user/get-started/why-you-provide-backups',
      'user/get-started/try-the-vault',
      'user/get-started/your-own-messages',
      'user/get-started/install-the-desktop-app',
    ],
  },
  {
    label: 'Prepare a backup',
    items: [
      'user/prepare-a-backup',
      'user/prepare-a-backup/iphone-ipad',
      'user/prepare-a-backup/iphone-whatsapp',
      'user/prepare-a-backup/android-sms',
      'user/prepare-a-backup/android-whatsapp',
    ],
  },
  'user/import-from-a-backup',
  'user/browse-your-messages',
  {
    label: 'How do I…',
    items: [
      'user/how-to/search',
      'user/how-to/contacts-and-labels',
      'user/how-to/saved-searches',
      'user/how-to/trash',
      'user/how-to/settings',
      'user/how-to/convert-formats',
      'user/how-to/extract-to-files',
      'user/how-to/export-from-the-vault',
      'user/how-to/media-and-privacy',
      { slug: 'user/how-to/rescue-imports', badge: limitedBadge },
      'user/how-to/update',
      'user/how-to/troubleshooting',
    ],
  },
  'user/glossary',
];

const developerItems = [
  'developer',
  'developer/run-from-source',
  'developer/docker-compose',
  {
    label: 'CLI tools',
    items: [
      'developer/reference/cli',
      'developer/reference/cli/imessage-ir-exporter',
      'developer/reference/cli/sms-backup-restore-exporter',
      'developer/reference/cli/whatsapp-exporter',
      'developer/reference/cli/message-reexporter',
      'developer/reference/cli/vault-push',
      'developer/reference/cli/vault-pull',
      'developer/reference/cli/go-sms-pro-exporter',
      'developer/reference/cli/imazing-exporter',
      'developer/reference/cli/openextract-exporter',
      'developer/reference/cli/sms-backup-plus-exporter',
    ],
  },
  'developer/reference/api',
  {
    label: 'Formats',
    items: [
      'developer/formats',
      'developer/formats/mail-archive',
      'developer/formats/sms-backup-restore-xml',
      'developer/formats/convert',
      {
        label: 'SMS Backup & Restore',
        items: [
          'developer/formats/sms-backup-restore/input',
          'developer/formats/sms-backup-restore/mapping',
        ],
      },
      {
        label: 'SMS Backup+',
        items: [
          'developer/formats/sms-backup-plus/format',
          'developer/formats/sms-backup-plus/mapping',
        ],
      },
      {
        label: 'GO SMS Pro',
        items: ['developer/formats/go-sms-pro/mapping'],
      },
      {
        label: 'iMazing',
        items: [
          'developer/formats/imazing/input',
          'developer/formats/imazing/design',
        ],
      },
    ],
  },
  {
    label: 'Instance internals',
    collapsed: true,
    items: [
      'developer/reference/config-and-accounts',
      'developer/reference/database',
      'developer/reference/export-structure',
      'developer/reference/csv-columns',
      'developer/reference/server-cli',
    ],
  },
];

export default defineConfig({
  site: 'https://vault.bitrealm.dev',
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
            link: '/user/',
            icon: 'open-book',
            items: userGuideItems,
          },
          {
            label: 'Developer',
            link: '/developer/',
            icon: 'laptop',
            items: developerItems,
          },
        ]),
      ],
    }),
  ],
});
