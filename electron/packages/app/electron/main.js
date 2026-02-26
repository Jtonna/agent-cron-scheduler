const { app, BrowserWindow, Tray, Menu, nativeImage } = require('electron');
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

let standaloneServerProcess = null;

/**
 * Start the Next.js standalone server.
 * Returns the port the server is listening on.
 */
async function startStandaloneServer() {
  const standaloneDir = path.join(__dirname, '..', 'standalone', 'packages', 'frontend');

  // Find a free port
  const port = await new Promise((resolve, reject) => {
    const tempServer = http.createServer();
    tempServer.listen(0, '127.0.0.1', () => {
      const freePort = tempServer.address().port;
      tempServer.close(() => resolve(freePort));
    });
    tempServer.on('error', reject);
  });

  // Spawn the standalone server using Electron's built-in Node.js runtime.
  // ELECTRON_RUN_AS_NODE=1 makes the Electron binary act as plain Node.js,
  // which is required because packaged apps don't have system `node` in PATH.
  const { spawn } = require('child_process');
  standaloneServerProcess = spawn(process.execPath, ['server.js'], {
    cwd: standaloneDir,
    env: {
      ...process.env,
      PORT: String(port),
      HOSTNAME: '127.0.0.1',
      ELECTRON_RUN_AS_NODE: '1',
    },
    stdio: 'pipe',
  });

  // Wait for the server to be ready
  await new Promise((resolve, reject) => {
    const timeout = setTimeout(() => {
      reject(new Error('Standalone server failed to start within 10 seconds'));
    }, 10000);

    const checkReady = () => {
      http.get(`http://127.0.0.1:${port}/`, { timeout: 1000 }, (res) => {
        if (res.statusCode === 200 || res.statusCode === 404) {
          clearTimeout(timeout);
          resolve();
        } else {
          setTimeout(checkReady, 100);
        }
      }).on('error', () => {
        setTimeout(checkReady, 100);
      });
    };
    checkReady();
  });

  console.log(`Standalone server running on http://127.0.0.1:${port}`);
  return port;
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

function getTrayIconPath() {
  // Try favicon from built frontend (production)
  const standaloneIcon = path.join(__dirname, '..', 'standalone', 'packages', 'frontend', 'public', 'favicon.ico');
  if (fs.existsSync(standaloneIcon)) return standaloneIcon;

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
    const standalonePort = await startStandaloneServer();
    frontendUrl = `http://127.0.0.1:${standalonePort}`;
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
  if (standaloneServerProcess) {
    standaloneServerProcess.kill();
  }
});

app.on('will-quit', () => {
  if (standaloneServerProcess) {
    standaloneServerProcess.kill();
  }
});
