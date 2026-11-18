// Serves app/dist as plain files over http://127.0.0.1:<free port> with a
// minimal static file server (Node's http module, no fallback to index.html),
// fetches the index, the script it names and a deep link, prints what came
// back, and stops. No browser is opened.
// Run from the spike folder: node tools/serve-check.mjs

import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { extname, join, normalize, sep, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..', 'app', 'dist');
const types = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.json': 'application/json',
  '.png': 'image/png',
  '.ico': 'image/x-icon',
};

const server = createServer(async (req, res) => {
  const path = decodeURIComponent(new URL(req.url, 'http://x').pathname);
  const file = normalize(join(root, path === '/' ? 'index.html' : path));
  if (!file.startsWith(root + sep)) {
    res.writeHead(403).end();
    return;
  }
  try {
    const body = await readFile(file);
    res.writeHead(200, { 'content-type': types[extname(file)] ?? 'application/octet-stream' }).end(body);
  } catch {
    res.writeHead(404, { 'content-type': 'text/plain' }).end('not found');
  }
});

await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
const base = `http://127.0.0.1:${server.address().port}`;
console.log(`serving the export at ${base.replace(/\d+$/, '<port>')}`);

async function get(path) {
  const r = await fetch(base + path);
  const body = await r.text();
  console.log(`GET ${path} -> ${r.status} ${r.headers.get('content-type')} ${Buffer.byteLength(body)} bytes`);
  return body;
}

try {
  const html = await get('/');
  const script = html.match(/<script src="([^"]+)"/)?.[1];
  console.log(`index names script ${script}`);
  if (script) {
    const js = await get(script);
    console.log(`script looks like the app: ${js.includes('give_minutes') && js.includes('Give 15 minutes')}`);
  }
  await get('/child/c1');
} finally {
  server.close();
}
