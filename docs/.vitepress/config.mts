import {defineConfig} from 'vitepress'

import {withMermaid} from 'vitepress-plugin-mermaid';


// https://vitepress.dev/reference/site-config
const config = defineConfig({
    title: "Hyperion",
    description: "The most advanced Minecraft game engine built in Rust",
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
                    {text: 'Architecture', link: '/guide/architecture'},
                ]
            }
        ],
        socialLinks: [
            {icon: 'github', link: 'https://github.com/vuejs/vitepress'}
        ]
    }
})


export default withMermaid(config);