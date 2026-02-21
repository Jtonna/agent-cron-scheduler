const { app, BrowserWindow } = require('electron');
const path = require('path');
const fs = require('fs');
const os = require('os');
const http = require('http');

const isDev = !app.isPackaged;

function getAcsBinaryPath() {
  const ext = process.platform === 'win32' ? '.exe' : '';
  if (isDev) {
    return path.join(__dirname, '..', '..', '..', '..', 'acs', 'target', 'release', `acs${ext}`);
  }
  return path.join(process.resourcesPath, 'acs-binary', `acs${ext}`);
}

function getDataDir() {
  if (process.platform === 'win32') {
    return path.join(
      process.env.LOCALAPPDATA || path.join(os.homedir(), 'AppData', 'Local'),
      'agent-cron-scheduler'
    );
  } else if (process.platform === 'darwin') {
    return path.join(os.homedir(), 'Library', 'Application Support', 'agent-cron-scheduler');
  } else {
    return path.join(
      process.env.XDG_DATA_HOME || path.join(os.homedir(), '.local', 'share'),
      'agent-cron-scheduler'
    );
  }
}

function ensureDataDir() {
  const dataDir = getDataDir();
  fs.mkdirSync(dataDir, { recursive: true });
}

function isDaemonRunning() {
  return new Promise((resolve) => {
    const req = http.get('http://127.0.0.1:8377/health', { timeout: 2000 }, (res) => {
      resolve(res.statusCode === 200);
    });
    req.on('error', () => resolve(false));
    req.on('timeout', () => {
      req.destroy();
      resolve(false);
    });
  });
}

function startStaticServer(outDir) {
  return new Promise((resolve) => {
    const mimeTypes = {
      '.html': 'text/html',
      '.js': 'text/javascript',
      '.css': 'text/css',
      '.json': 'application/json',
      '.png': 'image/png',
      '.jpg': 'image/jpeg',
      '.gif': 'image/gif',
      '.svg': 'image/svg+xml',
      '.ico': 'image/x-icon',
      '.woff': 'font/woff',
      '.woff2': 'font/woff2',
      '.ttf': 'font/ttf',
      '.txt': 'text/plain',
    };

    const server = http.createServer((req, res) => {
      let pathname = decodeURIComponent(new URL(req.url, 'http://localhost').pathname);
      if (pathname.startsWith('/')) pathname = pathname.substring(1);

      let filePath = path.join(outDir, pathname);
      const ext = path.extname(filePath);

      // If has extension, serve directly
      if (ext && fs.existsSync(filePath)) {
        const contentType = mimeTypes[ext] || 'application/octet-stream';
        const content = fs.readFileSync(filePath);
        res.writeHead(200, { 'Content-Type': contentType });
        res.end(content);
        return;
      }

      // SPA fallback for routes without extensions
      // Try {path}.html
      if (fs.existsSync(filePath + '.html')) {
        const content = fs.readFileSync(filePath + '.html');
        res.writeHead(200, { 'Content-Type': 'text/html' });
        res.end(content);
        return;
      }
      // Try {path}/index.html
      if (fs.existsSync(path.join(filePath, 'index.html'))) {
        const content = fs.readFileSync(path.join(filePath, 'index.html'));
        res.writeHead(200, { 'Content-Type': 'text/html' });
        res.end(content);
        return;
      }
      // Fallback to index.html (SPA)
      const indexPath = path.join(outDir, 'index.html');
      if (fs.existsSync(indexPath)) {
        const content = fs.readFileSync(indexPath);
        res.writeHead(200, { 'Content-Type': 'text/html' });
        res.end(content);
        return;
      }

      res.writeHead(404);
      res.end('Not Found');
    });

    // Listen on random available port
    server.listen(0, '127.0.0.1', () => {
      const port = server.address().port;
      console.log(`Static server running on http://127.0.0.1:${port}`);
      resolve(port);
    });
  });
}

async function startDaemon() {
  const running = await isDaemonRunning();
  if (running) {
    console.log('ACS daemon already running.');
    return;
  }

  const binaryPath = getAcsBinaryPath();
  if (!fs.existsSync(binaryPath)) {
    console.log('ACS binary not found; skipping daemon start');
    return;
  }

  const { spawn } = require('child_process');
  const child = spawn(binaryPath, ['start'], {
    detached: true,
    windowsHide: true,
    stdio: 'ignore',
  });
  child.unref();
  console.log('ACS daemon process launched.');
}

function createWindow(url) {
  const win = new BrowserWindow({
    width: 1200,
    height: 800,
    autoHideMenuBar: true,
    webPreferences: {
      preload: path.join(__dirname, 'preload.js'),
      nodeIntegration: false,
      contextIsolation: true,
      sandbox: false,
      additionalArguments: [
        `--data-dir=${getDataDir()}`,
        `--acs-binary=${getAcsBinaryPath()}`,
      ],
    },
  });

  win.setMenuBarVisibility(false);
  win.loadURL(url);
}

app.whenReady().then(async () => {
  ensureDataDir();
  await startDaemon();

  // Determine frontend URL
  let frontendUrl;
  if (isDev) {
    frontendUrl = 'http://localhost:3000';
  } else {
    // Serve static files via local HTTP server instead of a custom protocol.
    // Custom protocols like app:// break Next.js router internals because
    // Chromium doesn't treat them as valid URL origins for the URL constructor.
    const outDir = path.join(__dirname, '..', 'out');
    const staticPort = await startStaticServer(outDir);
    frontendUrl = `http://127.0.0.1:${staticPort}`;
  }

  createWindow(frontendUrl);

  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) createWindow(frontendUrl);
  });
});

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') app.quit();
});
