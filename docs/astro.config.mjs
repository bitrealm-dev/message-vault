import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';
import { starlightBasePath } from 'starlight-base-path';

const limitedBadge = {
  text: 'Limited',
  variant: 'caution',
};

export default defineConfig({
  site: 'https://bitrealm-dev.github.io',
  base: '/message-vault-io/',
  integrations: [
    starlight({
      title: 'Message Exporters',
      description:
        'Turn Apple and Android message backups into files you can keep and open.',
      plugins: [starlightBasePath()],
      editLink: {
        baseUrl:
          'https://github.com/bitrealm-dev/message-vault-io/edit/main/docs/',
      },
      social: [
        {
          icon: 'github',
          label: 'GitHub',
          href: 'https://github.com/bitrealm-dev/message-vault-io',
        },
      ],
      customCss: ['./src/styles/custom.css'],
      sidebar: [
        { label: 'Home', slug: '' },
        {
          label: 'Get started',
          items: [
            'get-started/install',
            'get-started/supported-formats',
            'get-started/first-export',
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
          label: 'Understand the output',
          collapsed: true,
          items: [
            'understand-output/export-structure',
            'understand-output/csv-columns',
          ],
        },
        {
          label: 'Command-line reference',
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
          ],
        },
      ],
    }),
  ],
});
