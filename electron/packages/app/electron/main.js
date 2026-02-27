const { app, BrowserWindow, Tray, Menu } = require('electron');
const path = require('path');
const fs = require('fs');
const os = require('os');
const http = require('http');

const isDev = !app.isPackaged;

let tray = null;

function getBinaryPath() {
  const ext = process.platform === 'win32' ? '.exe' : '';
  if (isDev) {
    return path.join(__dirname, '..', '..', '..', '..', 'acs', 'target', 'release', `agentcronsystem${ext}`);
  }
  return path.join(process.resourcesPath, 'agentcronsystem-binary', `agentcronsystem${ext}`);
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

async function startDaemon() {
  const running = await isDaemonRunning();
  if (running) {
    console.log('ACS daemon already running.');
    return;
  }

  const binaryPath = getBinaryPath();
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

function getOutDir() {
  return path.join(__dirname, '..', 'out');
}

const MIME_TYPES = {
  '.html': 'text/html; charset=utf-8',
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
  '.txt': 'text/plain',
  '.map': 'application/json',
};

/**
 * Start an in-process HTTP server that serves the React SPA
 * with static file resolution and SPA fallback.
 * Runs inside Electron's main process — no child process, no terminal windows.
 */
function startStaticServer(outDir) {
  return new Promise((resolve, reject) => {
    const server = http.createServer((req, res) => {
      const parsedUrl = new URL(req.url, 'http://localhost');
      let pathname = decodeURIComponent(parsedUrl.pathname);

      // Try exact file
      let filePath = path.join(outDir, pathname);
      if (tryServeFile(filePath, res)) return;

      // Try with /index.html appended (directory index)
      if (tryServeFile(path.join(filePath, 'index.html'), res)) return;

      // Try with .html extension
      if (tryServeFile(filePath + '.html', res)) return;

      // SPA fallback — serve index.html for all other requests
      // React Router will handle client-side routing
      serveFile(path.join(outDir, 'index.html'), res);
    });

    server.listen(0, '127.0.0.1', () => {
      const port = server.address().port;
      console.log(`Static server running on http://127.0.0.1:${port}`);
      resolve(port);
    });
    server.on('error', reject);
  });
}

function tryServeFile(filePath, res) {
  if (fs.existsSync(filePath) && fs.statSync(filePath).isFile()) {
    serveFile(filePath, res);
    return true;
  }
  return false;
}

function serveFile(filePath, res) {
  const ext = path.extname(filePath).toLowerCase();
  const contentType = MIME_TYPES[ext] || 'application/octet-stream';
  const content = fs.readFileSync(filePath);
  res.writeHead(200, { 'Content-Type': contentType });
  res.end(content);
}

function getTrayIconPath() {
  const outDir = getOutDir();
  const outIcon = path.join(outDir, 'favicon.ico');
  if (fs.existsSync(outIcon)) return outIcon;

  // Try from frontend source (dev mode)
  const srcIcon = path.join(__dirname, '..', '..', 'frontend', 'src', 'app', 'favicon.ico');
  if (fs.existsSync(srcIcon)) return srcIcon;

  // No icon found
  return null;
}

function createTray(win) {
  const iconPath = getTrayIconPath();
  if (!iconPath) {
    console.log('No tray icon found; skipping tray creation.');
    return;
  }

  tray = new Tray(iconPath);

  const contextMenu = Menu.buildFromTemplate([
    {
      label: 'Open ACS',
      click: () => { win.show(); win.focus(); }
    },
    { type: 'separator' },
    {
      label: 'Quit',
      click: () => {
        app.isQuitting = true;
        app.quit();
      }
    }
  ]);

  tray.setToolTip('Agent Cron Scheduler');
  tray.setContextMenu(contextMenu);

  // Double-click tray icon to restore window
  tray.on('double-click', () => {
    win.show();
    win.focus();
  });
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
        `--agentcronsystem-binary=${getBinaryPath()}`,
      ],
    },
  });

  win.setMenuBarVisibility(false);

  // Minimize to tray instead of closing
  win.on('close', (event) => {
    if (!app.isQuitting) {
      event.preventDefault();
      win.hide();
    }
  });

  win.loadURL(url);
  return win;
}

app.whenReady().then(async () => {
  ensureDataDir();
  await startDaemon();

  // Determine frontend URL
  let frontendUrl;
  if (isDev) {
    frontendUrl = 'http://localhost:3000';
  } else {
    const outDir = getOutDir();
    const port = await startStaticServer(outDir);
    frontendUrl = `http://127.0.0.1:${port}`;
  }

  const win = createWindow(frontendUrl);
  createTray(win);

  app.on('activate', () => {
    const allWindows = BrowserWindow.getAllWindows();
    if (allWindows.length === 0) {
      const newWin = createWindow(frontendUrl);
      createTray(newWin);
    } else {
      allWindows[0].show();
      allWindows[0].focus();
    }
  });
});

app.on('window-all-closed', () => {
  // Don't quit — app lives in system tray
});

app.on('before-quit', () => {
  app.isQuitting = true;
});
