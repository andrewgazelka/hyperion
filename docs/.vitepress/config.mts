import {defineConfig} from 'vitepress'

import {withMermaid} from 'vitepress-plugin-mermaid';

import footnote from 'markdown-it-footnote'

// https://vitepress.dev/reference/site-config
const config = defineConfig({
    title: "Hyperion",
    description: "The most advanced Minecraft game engine built in Rust",
    base: "/hyperion/",
    markdown: {
        math: true,
        config: (md) => {
            md.use(footnote)
        }
    },
    themeConfig: {
        // https://vitepress.dev/reference/default-theme-config
        nav: [
            {text: 'Home', link: '/'},
            {text: 'Guide', link: '/guide'},
        ],

        sidebar: [
            {
                text: 'Guide',
                items: [
                    {text: 'Introduction', link: '/guide/introduction'},
                    {text: 'Game Server', link: '/guide/game-server'},
                ]
            }
        ],
        socialLinks: [
            {icon: 'github', link: 'https://github.com/vuejs/vitepress'}
        ]
    }
})


export default withMermaid(config);