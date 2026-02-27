// import '@fluentui/web-components/'
import { setTheme } from '@fluentui/web-components'

import { webLightTheme } from '@fluentui/tokens'
import './style.css'
import { renderApp } from './app.js'

setTheme(webLightTheme)
renderApp()
