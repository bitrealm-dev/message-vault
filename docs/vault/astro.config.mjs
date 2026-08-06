import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';
import { starlightBasePath } from 'starlight-base-path';

export default defineConfig({
  site: 'https://bitrealm-dev.github.io',
  base: '/message-vault-rs/',
  integrations: [
    starlight({
      title: 'Message Vault',
      description:
        'Keep your text-message history in a local SQLite vault and browse it in a website you control.',
      plugins: [starlightBasePath()],
      editLink: {
        baseUrl:
          'https://github.com/bitrealm-dev/message-vault-rs/edit/main/docs/',
      },
      social: [
        {
          icon: 'github',
          label: 'GitHub',
          href: 'https://github.com/bitrealm-dev/message-vault-rs',
        },
      ],
      customCss: ['./src/styles/custom.css'],
      sidebar: [
        { label: 'Home', slug: '' },
        {
          label: 'Get started',
          items: [
            'get-started/install',
            'get-started/docker',
            'get-started/try-the-demo',
            'get-started/first-personal-import',
          ],
        },
        {
          label: 'Import',
          items: [
            'import/from-message-exporters',
            'import/same-machine',
            'import/modes-and-dedupe',
            'import/http-api',
          ],
        },
        {
          label: 'Browse',
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
            'reference/config-and-accounts',
            'reference/message-ir',
            'reference/database',
          ],
        },
      ],
    }),
  ],
});
