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
    label: 'Introduction',
    items: [
      'introduction/what-is-message-vault',
      'introduction/why-manual-backups',
      'introduction/quick-start',
      'introduction/install',
      'introduction/glossary',
    ],
  },
  {
    label: 'Prepare your backups',
    items: [
      'prepare-your-backups/iphone-ipad',
      'prepare-your-backups/iphone-whatsapp',
      'prepare-your-backups/android-sms',
      'prepare-your-backups/android-whatsapp',
      { slug: 'prepare-your-backups/rescue-imports', badge: limitedBadge },
    ],
  },
  {
    label: 'Set up the server',
    items: [
      'set-up-the-server/docker-install',
      'set-up-the-server/first-personal-vault',
      'set-up-the-server/try-the-demo',
      'set-up-the-server/updating',
    ],
  },
  {
    label: 'Use the desktop app',
    items: [
      'use-the-desktop-app/extract-messages',
      'use-the-desktop-app/convert-formats',
      'use-the-desktop-app/contacts',
      'use-the-desktop-app/import-into-vault',
      'use-the-desktop-app/export-from-vault',
      'use-the-desktop-app/media-and-privacy',
      'use-the-desktop-app/output-formats',
    ],
  },
  {
    label: 'Browse the vault',
    items: [
      'browse/navigation-and-sources',
      'browse/search',
      'browse/contacts-and-labels',
      'browse/group-messages',
      'browse/trash-and-undo',
      'browse/settings',
    ],
  },
  {
    label: 'Reference',
    collapsed: true,
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
      'reference/server-cli',
      'reference/config-and-accounts',
      'reference/api',
      'reference/database',
      'reference/export-structure',
      'reference/csv-columns',
      'troubleshooting',
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
            label: 'Format Reference',
            link: '/formats/',
            icon: 'document',
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
                items: [
                  'formats/imazing/input',
                  'formats/imazing/design',
                ],
              },
            ],
          },
        ]),
      ],
    }),
  ],
});
