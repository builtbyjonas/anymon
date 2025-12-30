const express = require('express');
const path = require('path');
const serverless = require('serverless-http');

const app = express();

const DEFAULT_REPO_URL = 'https://github.com/builtbyjonas/anymon';
const REPO_URL = process.env.REPOSITORY_URL || DEFAULT_REPO_URL;
const BRANCH = process.env.REPOSITORY_BRANCH || 'main';
const RAW_OVERRIDE = process.env.REPOSITORY_RAW_URL || '';

function getRawBase() {
  if (RAW_OVERRIDE) return RAW_OVERRIDE.replace(/\/+$/g, '');
  const m = REPO_URL.match(/github\.com[:\/](.+?)\/([^\/]+?)(?:\.git)?$/i);
  if (m) {
    const owner = m[1];
    const repo = m[2];
    return `https://raw.githubusercontent.com/${owner}/${repo}/${BRANCH}`;
  }
  return `https://raw.githubusercontent.com/builtbyjonas/anymon/${BRANCH}`;
}

const RAW_BASE = getRawBase();

app.get('/', (req, res) => res.redirect(REPO_URL));

async function fetchAndSend(res, filename) {
  const filePath = `installers/${filename}`;
  const url = `${RAW_BASE}/${filePath}`;
  try {
    const resp = await fetch(url);
    console.log(`Fetching ${url} - ${resp.status}`);
    if (!resp.ok) return res.status(404).type('text/plain').send('Not found');
    const text = await resp.text();
    res.type('text/plain').send(text);
  } catch (err) {
    res.status(502).type('text/plain').send('Failed to fetch file');
  }
}

app.get('/install.ps1', (req, res) => fetchAndSend(res, 'install.ps1'));
app.get('/install.sh', (req, res) => fetchAndSend(res, 'install.sh'));

app.use(express.static(path.join(__dirname, '..')));

if (require.main === module) {
  const port = process.env.PORT || 3000;
  app.listen(port, () => console.log(`anymon-www listening on http://localhost:${port}`));
}

module.exports = serverless(app);
