// TilDown service worker: makes the web app installable and usable offline.
// Same-origin GETs are served from the cache and refreshed in the background
// (stale-while-revalidate), so a new build shows up on the next launch.
// Requests to the sync server (other origin, POST) are never touched.
const CACHE = 'tildown-v1';
const SHELL = ['./', './index.html', './manifest.webmanifest', './icons/icon-192.png',
  './pkg/tildown.js', './pkg/tildown_bg.wasm'];

self.addEventListener('install', (e) => {
  e.waitUntil(caches.open(CACHE).then((c) => c.addAll(SHELL)).then(() => self.skipWaiting()));
});

self.addEventListener('activate', (e) => {
  e.waitUntil(caches.keys()
    .then((ks) => Promise.all(ks.filter((k) => k !== CACHE).map((k) => caches.delete(k))))
    .then(() => self.clients.claim()));
});

self.addEventListener('fetch', (e) => {
  const req = e.request;
  if (req.method !== 'GET' || new URL(req.url).origin !== location.origin) return;
  e.respondWith(caches.open(CACHE).then(async (c) => {
    const hit = await c.match(req, { ignoreSearch: true });
    const fresh = fetch(req).then((r) => {
      if (r.ok) c.put(req, r.clone());
      return r;
    }).catch(() => hit);
    if (hit) {
      e.waitUntil(fresh); // keep the worker alive until the refresh is stored
      return hit;
    }
    return fresh;
  }));
});
