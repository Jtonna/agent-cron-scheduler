const { contextBridge } = require('electron');
const path = require('path');
const fs = require('fs');

// Paths are passed from main.js via webPreferences.additionalArguments
// because process.resourcesPath is not available in preload context.
function getArg(name) {
  const prefix = `--${name}=`;
  const arg = process.argv.find(a => a.startsWith(prefix));
  return arg ? arg.slice(prefix.length) : null;
}

const dataDir = getArg('data-dir');
const acsBinaryPath = getArg('agentcronsystem-binary');

function getDaemonPort() {
  if (!dataDir) return null;
  try {
    return fs.readFileSync(path.join(dataDir, 'agentcronsystem.port'), 'utf8').trim() || null;
  } catch {
    return null;
  }
}

contextBridge.exposeInMainWorld('acs', {
  getDaemonPort: () => getDaemonPort(),

  getDaemonUrl: () => {
    const port = getDaemonPort();
    return port ? `http://127.0.0.1:${port}` : null;
  },

  getDataDir: () => dataDir,

  installService: () => {
    if (!acsBinaryPath) return { success: false, message: 'ACS binary path not available' };
    const { execFileSync } = require('child_process');
    try {
      const output = execFileSync(acsBinaryPath, ['start'], {
        encoding: 'utf8',
        timeout: 15000,
      });
      return { success: true, message: output };
    } catch (err) {
      return { success: false, message: err.stderr || err.message };
    }
  },

  uninstallService: () => {
    if (!acsBinaryPath) return { success: false, message: 'ACS binary path not available' };
    const { execFileSync } = require('child_process');
    try {
      const output = execFileSync(acsBinaryPath, ['uninstall'], {
        encoding: 'utf8',
        timeout: 15000,
      });
      return { success: true, message: output };
    } catch (err) {
      return { success: false, message: err.stderr || err.message };
    }
  },

  getServiceStatus: () => {
    if (!acsBinaryPath) return { success: false, message: 'ACS binary path not available' };
    const { execFileSync } = require('child_process');
    try {
      const output = execFileSync(acsBinaryPath, ['status'], {
        encoding: 'utf8',
        timeout: 10000,
      });
      return { success: true, message: output };
    } catch (err) {
      return { success: false, message: err.stderr || err.message };
    }
  },
});
