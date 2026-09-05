// Window logic.
//
// The Rust side does the searching; this file collects the form, sends it, and
// draws what comes back. Everything is built with DOM calls rather than
// innerHTML: titles and store names are text from AliExpress, and pasting them
// into HTML would let a listing run script in the window.

const { invoke } = window.__TAURI__.core

const form = document.getElementById('form')
const keywordEl = document.getElementById('keyword')
const searchButton = document.getElementById('search')
const resultsEl = document.getElementById('results')
const statusEl = document.getElementById('status')

const fields = {
  shopOnly: document.getElementById('shop-only'),
  shopExclude: document.getElementById('shop-exclude'),
  choice: document.getElementById('choice'),
  freeShipping: document.getElementById('free-shipping'),
  excludeAds: document.getElementById('exclude-ads'),
  dedupe: document.getElementById('dedupe'),
  minPrice: document.getElementById('min-price'),
  maxPrice: document.getElementById('max-price'),
  minRating: document.getElementById('min-rating'),
  sort: document.getElementById('sort'),
  pages: document.getElementById('pages'),
  limit: document.getElementById('limit'),
}

let busy = false

function setStatus(text, kind) {
  statusEl.textContent = text
  statusEl.dataset.kind = kind || 'info'
}

function setBusy(value) {
  busy = value
  searchButton.disabled = value
  searchButton.textContent = value ? 'Searching…' : 'Search'
}

function numberOrNull(input) {
  const value = input.value.trim()
  if (value === '') return null
  const number = Number(value)
  return Number.isFinite(number) ? number : null
}

function query() {
  return {
    keyword: keywordEl.value,
    kind: fields.shopOnly.checked ? 'hundred_yen_shop' : fields.shopExclude.checked ? 'normal' : 'any',
    choice: fields.choice.checked,
    free_shipping: fields.freeShipping.checked,
    min_price: numberOrNull(fields.minPrice),
    max_price: numberOrNull(fields.maxPrice),
    min_rating: numberOrNull(fields.minRating),
    exclude_ads: fields.excludeAds.checked,
    dedupe: fields.dedupe.checked,
    sort: fields.sort.value,
    pages: Math.max(1, Math.min(10, Number(fields.pages.value) || 1)),
    limit: numberOrNull(fields.limit),
  }
}

function fillForm(initial) {
  keywordEl.value = initial.keyword
  fields.shopOnly.checked = initial.kind === 'hundred_yen_shop'
  fields.shopExclude.checked = initial.kind === 'normal'
  fields.choice.checked = initial.choice
  fields.freeShipping.checked = initial.free_shipping
  fields.excludeAds.checked = initial.exclude_ads
  fields.dedupe.checked = initial.dedupe
  fields.minPrice.value = initial.min_price ?? ''
  fields.maxPrice.value = initial.max_price ?? ''
  fields.minRating.value = initial.min_rating ?? ''
  fields.sort.value = initial.sort
  fields.pages.value = String(initial.pages)
  fields.limit.value = initial.limit ?? ''
}

// Per product rather than per site: the cookies a user passes in can switch
// the currency the server renders, and the product carries what it was given.
function money(amount, currency) {
  if (currency === 'JPY') return `¥${Math.round(amount).toLocaleString()}`
  return `${amount.toFixed(2)} ${currency}`
}

function render(results) {
  resultsEl.replaceChildren()
  if (results.products.length === 0) {
    const empty = document.createElement('p')
    empty.className = 'empty'
    empty.textContent = 'No products matched.'
    resultsEl.append(empty)
    return
  }
  results.products.forEach((product, index) => {
    resultsEl.append(renderCard(product, results.scores[index]))
  })
}

function renderCard(product, score) {
  const card = document.createElement('article')
  card.className = 'card'
  card.title = product.title
  card.addEventListener('click', () => openProduct(product.url))

  const image = document.createElement('img')
  image.src = product.image_url
  image.alt = ''
  image.loading = 'lazy'
  card.append(image)

  const body = document.createElement('div')
  body.className = 'body'

  const tags = document.createElement('div')
  tags.className = 'tags'
  if (product.hundred_yen_shop) tags.append(tag('100-yen Shop', 'hundred_yen_shop'))
  if (product.choice) tags.append(tag('Choice', 'choice'))
  if (product.ad) tags.append(tag('Ad', 'ad'))
  body.append(tags)

  const price = document.createElement('div')
  price.className = 'price'
  price.textContent = money(product.price, product.currency)
  if (product.original_price && product.original_price > product.price) {
    const was = document.createElement('span')
    was.className = 'was'
    was.textContent = money(product.original_price, product.currency)
    price.append(was)
  }
  body.append(price)

  const meta = document.createElement('div')
  meta.className = 'meta'
  const parts = []
  parts.push(product.rating === null ? '★ –' : `★ ${product.rating.toFixed(1)}`)
  // The count rather than the card's own text: the text is in the site's
  // language, and the window is in English.
  if (product.sales !== null) parts.push(`${product.sales.toLocaleString()} sold`)
  if (score > 0) parts.push(`value ${score.toFixed(2)}`)
  meta.textContent = parts.join(' · ')
  body.append(meta)

  if (product.bulk_offer) {
    const offer = document.createElement('div')
    offer.className = 'offer'
    offer.textContent = product.bulk_offer
    body.append(offer)
  }

  const title = document.createElement('div')
  title.className = 'title'
  title.textContent = product.title
  body.append(title)

  card.append(body)
  return card
}

function tag(text, kind) {
  const el = document.createElement('span')
  el.className = `tag ${kind}`
  el.textContent = text
  return el
}

async function openProduct(url) {
  try {
    await invoke('open_product', { url })
  } catch (error) {
    setStatus(String(error), 'error')
  }
}

function summary(results) {
  return (
    `${results.products.length} products shown, ${results.fetched} fetched, ` +
    `${results.total_results.toLocaleString()} results on AliExpress.`
  )
}

async function runSearch() {
  if (busy) return
  const q = query()
  if (q.keyword.trim() === '') {
    setStatus('Type something to search for.', 'error')
    keywordEl.focus()
    return
  }
  setBusy(true)
  setStatus(`Searching for “${q.keyword.trim()}”…`)
  try {
    const results = await invoke('run_search', { query: q })
    render(results)
    setStatus(summary(results))
  } catch (error) {
    setStatus(String(error), 'error')
  } finally {
    setBusy(false)
  }
}

// The two programme boxes are one choice: only, or none, or either.
fields.shopOnly.addEventListener('change', () => {
  if (fields.shopOnly.checked) fields.shopExclude.checked = false
})
fields.shopExclude.addEventListener('change', () => {
  if (fields.shopExclude.checked) fields.shopOnly.checked = false
})

form.addEventListener('submit', (event) => {
  event.preventDefault()
  runSearch()
})

async function start() {
  try {
    const initial = await invoke('initial_form')
    fillForm(initial)
    if (initial.results) {
      // Fetched by the command line before the window opened: draw it as-is.
      // The form is filled in so the search can be refined from here.
      render(initial.results)
      setStatus(summary(initial.results))
    } else if (initial.keyword.trim() !== '') {
      setStatus(`Ready. Searching ${initial.site_host}.`)
      runSearch()
    } else {
      setStatus(`Ready. Searching ${initial.site_host}.`)
    }
  } catch (error) {
    setStatus(`Could not read the initial form: ${error}`, 'error')
  }
}

start()
