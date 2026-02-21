const { app, BrowserWindow, protocol, net } = require('electron');
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

function createWindow() {
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

  if (isDev) {
    win.loadURL('http://localhost:3000');
  } else {
    win.loadURL('app://./index.html');
  }
}

app.whenReady().then(async () => {
  ensureDataDir();
  await startDaemon();

  // Register custom app:// protocol for SPA fallback routing.
  // Without this, file:// protocol breaks client-side navigation
  // (e.g. /logs resolves to file:///C:/logs instead of the SPA).
  protocol.handle('app', (request) => {
    const url = new URL(request.url);
    let pathname = decodeURIComponent(url.pathname);
    // Remove leading slash for path.join
    if (pathname.startsWith('/')) pathname = pathname.substring(1);

    const outDir = path.join(__dirname, '..', 'out');
    let filePath = path.join(outDir, pathname);

    // If the path has a file extension, serve it directly
    if (path.extname(filePath)) {
      if (fs.existsSync(filePath)) {
        return net.fetch('file://' + filePath);
      }
      // File not found — return 404
      return new Response('Not Found', { status: 404 });
    }

    // No extension — try route resolution (SPA fallback)
    // 1. Try {path}.html
    if (fs.existsSync(filePath + '.html')) {
      return net.fetch('file://' + filePath + '.html');
    }
    // 2. Try {path}/index.html
    if (fs.existsSync(path.join(filePath, 'index.html'))) {
      return net.fetch('file://' + path.join(filePath, 'index.html'));
    }
    // 3. SPA fallback — serve index.html for client-side routing
    return net.fetch('file://' + path.join(outDir, 'index.html'));
  });

  createWindow();

  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) createWindow();
  });
});

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') app.quit();
});
