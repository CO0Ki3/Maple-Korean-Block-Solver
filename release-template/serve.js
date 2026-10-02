// 한글 모아모아 솔버 v2 — 의존성 없는 로컬 서버. Node.js 18 이상이면 됩니다.
// 사용: node serve.js   (또는 start.bat / start.command 더블클릭)
'use strict';
const http = require('http');
const fs = require('fs');
const path = require('path');
const { exec } = require('child_process');

const major = Number(process.versions.node.split('.')[0]);
if (major < 18) {
  console.error(`Node.js 18 이상이 필요합니다. 현재 버전: ${process.versions.node}. https://nodejs.org 에서 LTS를 설치하세요.`);
  process.exit(1);
}

const ROOT = path.join(__dirname, 'app');
const MIME = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.mjs': 'text/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.json': 'application/json; charset=utf-8',
  '.wasm': 'application/wasm',
  '.png': 'image/png',
  '.svg': 'image/svg+xml',
  '.ico': 'image/x-icon',
  '.map': 'application/json',
  '.woff2': 'font/woff2',
  '.txt': 'text/plain; charset=utf-8',
};

function handle(req, res) {
  let p;
  try {
    p = decodeURIComponent(new URL(req.url, 'http://localhost').pathname);
  } catch {
    res.writeHead(400);
    return res.end();
  }
  if (p === '/') p = '/index.html';
  const file = path.normalize(path.join(ROOT, p));
  if (!file.startsWith(ROOT)) {
    res.writeHead(403);
    return res.end();
  }
  fs.readFile(file, (err, data) => {
    if (err) {
      res.writeHead(404, { 'Content-Type': 'text/plain; charset=utf-8' });
      return res.end('Not found');
    }
    res.writeHead(200, { 'Content-Type': MIME[path.extname(file).toLowerCase()] || 'application/octet-stream', 'Cache-Control': 'no-cache' });
    res.end(data);
  });
}

function openBrowser(url) {
  const cmd = process.platform === 'win32' ? `start "" "${url}"` : process.platform === 'darwin' ? `open "${url}"` : `xdg-open "${url}"`;
  exec(cmd, () => {});
}

function listen(port, tries) {
  const server = http.createServer(handle);
  server.on('error', (e) => {
    if (e.code === 'EADDRINUSE' && tries > 0) listen(port + 1, tries - 1);
    else {
      console.error('서버를 열지 못했습니다:', e.message);
      process.exit(1);
    }
  });
  server.listen(port, '127.0.0.1', () => {
    const url = `http://localhost:${port}/`;
    console.log('');
    console.log(`  한글 모아모아 솔버 v2 가 열렸습니다: ${url}`);
    console.log('  브라우저가 자동으로 열리지 않으면 위 주소를 직접 여세요. 끝낼 때는 이 창을 닫거나 Ctrl+C.');
    console.log('');
    if (!process.env.NO_OPEN) openBrowser(url);
  });
}

if (!fs.existsSync(path.join(ROOT, 'index.html'))) {
  console.error(`app/index.html 이 없습니다. zip을 통째로 풀었는지 확인하세요. (경로: ${ROOT})`);
  process.exit(1);
}
listen(Number(process.env.PORT) || 8787, 10);
