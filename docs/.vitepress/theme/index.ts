// .vitepress/theme/index.js
import DefaultTheme from 'vitepress/theme'

import GithubSnippet from './components/GithubSnippet.vue'
import './custom.css'

// import {NolebaseInlineLinkPreviewPlugin} from '@nolebase/vitepress-plugin-inline-link-preview/client';
import '@fontsource/fira-code'

import { onMounted } from 'vue';

// export default DefaultTheme

import mediumZoom from 'medium-zoom';

export default {
    ...DefaultTheme,
    enhanceApp({app}) {
        app.component('GithubSnippet', GithubSnippet)
    },
    setup() {
        onMounted(() => {
            mediumZoom('[data-zoomable]', { background: 'var(--vp-c-bg)' });
        });
    },
};