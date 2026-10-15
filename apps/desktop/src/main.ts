import { mount } from 'svelte'
import './app.css'
import App from './App.svelte'

const app = mount(App, {
  target: document.getElementById('app')!,
})

// Drop the inline boot splash (index.html) once the app has painted a frame.
// Two rAFs wait for a real paint; the timeout is a fallback for the case where
// the opacity transition never fires (e.g. a reduced-motion override).
const splash = document.getElementById('splash')
if (splash) {
  requestAnimationFrame(() =>
    requestAnimationFrame(() => {
      splash.classList.add('hide')
      const remove = () => splash.remove()
      splash.addEventListener('transitionend', remove, { once: true })
      window.setTimeout(remove, 400)
    }),
  )
}

export default app
