import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';
import { starlightBasePath } from 'starlight-base-path';

const limitedBadge = {
  text: 'Limited',
  variant: 'caution',
};

export default defineConfig({
  site: 'https://bitrealm-dev.github.io',
  base: '/message-vault/',
  integrations: [
    starlight({
      title: 'Message Vault',
      description:
        'Extract messages from phone backups, import them into a local vault, and browse them in a website you control.',
      plugins: [starlightBasePath()],
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
      sidebar: [
        { label: 'Home', slug: '' },
        {
          label: 'Get started',
          items: [
            'get-started/install',
            'get-started/server-install',
            'get-started/docker',
            'get-started/try-the-demo',
            'get-started/first-export',
            'get-started/first-personal-import',
            'get-started/supported-formats',
          ],
        },
        {
          label: 'Apple',
          items: [
            'apple',
            'apple/text-messages',
            'apple/whatsapp',
          ],
        },
        {
          label: 'Android',
          items: [
            'android',
            'android/text-messages',
            'android/whatsapp',
          ],
        },
        {
          label: 'Other app exports',
          collapsed: true,
          badge: limitedBadge,
          items: [
            'other-app-exports',
            { slug: 'other-app-exports/go-sms-pro', badge: limitedBadge },
            { slug: 'other-app-exports/imazing', badge: limitedBadge },
            { slug: 'other-app-exports/openextract', badge: limitedBadge },
            { slug: 'other-app-exports/sms-backup-plus', badge: limitedBadge },
          ],
        },
        {
          label: 'Work with exports',
          items: [
            'work-with-exports/output-formats',
            'work-with-exports/convert-format',
            'work-with-exports/import-to-vault',
            'work-with-exports/contacts',
            'work-with-exports/media-and-privacy',
          ],
        },
        {
          label: 'Import to vault',
          items: [
            'import/from-message-exporters',
            'import/same-machine',
            'import/modes-and-dedupe',
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
          label: 'Understand the output',
          collapsed: true,
          items: [
            'understand-output/export-structure',
            'understand-output/csv-columns',
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
            'reference/cli/go-sms-pro-exporter',
            'reference/cli/imazing-exporter',
            'reference/cli/openextract-exporter',
            'reference/cli/sms-backup-plus-exporter',
            'reference/server-cli',
            'reference/config-and-accounts',
            'reference/message-ir',
            'reference/database',
            'reference/api',
          ],
        },
      ],
    }),
  ],
});
