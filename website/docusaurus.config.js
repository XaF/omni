// @ts-check
// Note: type annotations allow type checking and IDEs autocompletion

// const lightCodeTheme = require('prism-react-renderer/themes/github');
// const darkCodeTheme = require('prism-react-renderer/themes/dracula');

const lightCodeTheme = require('prism-react-renderer').themes.github;
const darkCodeTheme = require('prism-react-renderer').themes.vsDark;

/** @type {import('@docusaurus/types').Config} */
const config = {
  title: 'omni',
  tagline: 'the omnipotent dev tool',
  favicon: 'img/omni-logo.png',

  // Set the production url of your site here
  url: 'https://omnicli.dev',
  // Set the /<baseUrl>/ pathname under which your site is served
  // For GitHub pages deployment, it is often '/<projectName>/'
  baseUrl: '/',

  trailingSlash: false,

  // GitHub pages deployment config.
  // If you aren't using GitHub pages, you don't need these.
  organizationName: 'xaf', // Usually your GitHub org/user name.
  projectName: 'omni', // Usually your repo name.

  onBrokenLinks: 'throw',
  onBrokenMarkdownLinks: 'warn',

  // Even if you don't use internalization, you can use this field to set useful
  // metadata like html lang. For example, if your site is Chinese, you may want
  // to replace "en" with "zh-Hans".
  i18n: {
    defaultLocale: 'en',
    locales: ['en'],
  },

  presets: [
    [
      'classic',
      /** @type {import('@docusaurus/preset-classic').Options} */
      ({
        docs: {
          routeBasePath: '/',
          path: 'contents',
          sidebarPath: require.resolve('./sidebars.js'),
          // Please change this to your repo.
          // Remove this to remove the "edit this page" links.
          editUrl:
            'https://github.com/XaF/omni/tree/main/website/',
        },
        blog: {
          showReadingTime: true,
          // Please change this to your repo.
          // Remove this to remove the "edit this page" links.
          editUrl:
            'https://github.com/XaF/omni/tree/main/website/',
        },
        theme: {
          customCss: require.resolve('./src/css/custom.css'),
        },
      }),
    ],
  ],

  // plugins: [
    // [
      // '@docusaurus/plugin-content-docs',
      // {
        // id: 'tutorials',
        // path: 'tutorials',
        // routeBasePath: 'tutorials',
        // sidebarPath: require.resolve('./sidebars.js'),
      // },
    // ],
  // ],

  themeConfig:
    /** @type {import('@docusaurus/preset-classic').ThemeConfig} */
    ({
      // Replace with your project's social card
      image: 'img/omni-social-card.jpg',
      navbar: {
        title: 'omni',
        logo: {
          alt: 'omni',
          src: 'img/omni-logo.svg',
        },
        items: [
          {
            type: 'docSidebar',
            sidebarId: 'tutorialsSidebar',
            position: 'left',
            label: 'Tutorials',
          },
          {
            type: 'docSidebar',
            sidebarId: 'referenceSidebar',
            position: 'left',
            label: 'Reference',
          },
          //{
          //  // To version: npm run docusaurus docs:version <version>
          //  type: 'docsVersionDropdown',
          //  position: 'right',
          //},
          {
            href: 'https://github.com/XaF/omni/releases',
            className: 'header-github-release',
            value: '<img src="https://img.shields.io/github/v/release/XaF/omni?logo=github&sort=semver" alt="GitHub Release" />',
            position: 'right',
          },
          {
            href: 'https://github.com/XaF/omni',
            className: 'header-github-link',
            'aria-label': 'GitHub',
            position: 'right',
          },
        ],
      },
      algolia: {
        appId: '42CGPL0NK3',
        apiKey: 'dd9a0cfdc1189094accbfbacb6c7046a',
        indexName: 'omnicli',
        insights: true,
        contextualSearch: true,
        searchParameters: {},
        searchPagePath: 'search',
      },
      footer: {
        style: 'dark',
        // links: [
          // {
            // title: 'Tutorials',
            // items: [
              // {
                // label: 'Get Started',
                // to: '/tutorials/get-started',
              // },
              // {
                // label: 'Set up a repository',
                // to: '/tutorials/set-up-a-repository',
              // },
            // ],
          // },
          // // {
            // // title: 'Community',
            // // items: [
              // // {
                // // label: 'Stack Overflow',
                // // href: 'https://stackoverflow.com/questions/tagged/omni',
              // // },
              // // {
                // // label: 'Discord',
                // // href: 'https://discordapp.com/invite/omni',
              // // },
              // // {
                // // label: 'Twitter',
                // // href: 'https://twitter.com/raphaelbeamonte',
              // // },
            // // ],
          // // },
          // {
            // title: 'More',
            // items: [
              // // {
                // // label: 'Blog',
                // // to: '/blog',
              // // },
              // {
                // label: 'GitHub',
                // href: 'https://github.com/xaf/omni',
              // },
              // {
                // label: 'Issues',
                // href: 'https://github.com/xaf/omni/issues',
              // },
            // ],
          // },
        // ],
        copyright: `Copyright © ${new Date().getFullYear()} <a href="https://raphaelbeamonte.com">Raphaël Beamonte</a>. Built with Docusaurus.`,
      },
      prism: {
        additionalLanguages: [
          'bash',
          'json',
          'lisp',
          'lua',
          'makefile',
          'rust',
          'shell-session',
          'vim',
          'yaml',
        ],
        magicComments: [
          {
            className: 'theme-code-block-highlighted-line',
            line: 'highlight-next-line',
            block: {start: 'highlight-start', end: 'highlight-end'},
          },
          {
            className: 'code-block-error-line',
            line: 'This will error',
          },
        ],
        theme: lightCodeTheme,
        darkTheme: darkCodeTheme,
      },

      colorMode: {
        defaultMode: 'light',
        disableSwitch: false,
        respectPrefersColorScheme: true,
      },
      tableOfContents: {
        minHeadingLevel: 2,
        maxHeadingLevel: 5,
      },
    }),
};

module.exports = config;
