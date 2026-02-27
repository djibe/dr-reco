// import '@fluentui/web-components/'
import 'https://unpkg.com/@fluentui/web-components@3.0.0-rc.7/dist/web-components.min.js'
import { setTheme } from '@fluentui/web-components'
import { webLightTheme } from '@fluentui/tokens'
import './style.css'
import { renderApp } from './app.js'

setTheme(webLightTheme)
renderApp()
