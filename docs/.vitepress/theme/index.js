// .vitepress/theme/index.js
import DefaultTheme from 'vitepress/theme'

import GithubSnippet from './components/GithubSnippet.vue'
import './custom.css'

// import {NolebaseInlineLinkPreviewPlugin} from '@nolebase/vitepress-plugin-inline-link-preview/client';


// export default DefaultTheme

export default {
    ...DefaultTheme,
    enhanceApp({app}) {
        app.component('GithubSnippet', GithubSnippet)
    }
};