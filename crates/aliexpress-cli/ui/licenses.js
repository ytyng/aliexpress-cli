// The licenses window: shows THIRD-PARTY-NOTICES.txt, which the Rust side
// embeds in the binary. Set as text, never as HTML.

const { invoke } = window.__TAURI__.core

const noticesEl = document.getElementById('notices')

invoke('third_party_notices')
  .then((text) => {
    noticesEl.textContent = text
  })
  .catch((error) => {
    noticesEl.textContent = `Cannot load the licenses: ${error}`
  })
