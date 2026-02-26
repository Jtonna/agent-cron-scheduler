const fs = require('fs');
const path = require('path');

function copyDir(src, dest) {
  fs.mkdirSync(dest, { recursive: true });
  for (const entry of fs.readdirSync(src, { withFileTypes: true })) {
    const srcPath = path.join(src, entry.name);
    const destPath = path.join(dest, entry.name);
    if (entry.isDirectory()) {
      copyDir(srcPath, destPath);
    } else {
      fs.copyFileSync(srcPath, destPath);
    }
  }
}

// Copy standalone directory
const standaloneDir = path.join(__dirname, '..', 'packages', 'frontend', '.next', 'standalone');
const destDir = path.join(__dirname, '..', 'packages', 'app', 'standalone');

console.log('Copying standalone server...');
copyDir(standaloneDir, destDir);

// Copy static files into the frontend subdirectory
const staticDir = path.join(__dirname, '..', 'packages', 'frontend', '.next', 'static');
const destStaticDir = path.join(destDir, 'packages', 'frontend', '.next', 'static');

console.log('Copying static files...');
copyDir(staticDir, destStaticDir);

// Copy public files into the frontend subdirectory
const publicDir = path.join(__dirname, '..', 'packages', 'frontend', 'public');
const destPublicDir = path.join(destDir, 'packages', 'frontend', 'public');

console.log('Copying public files...');
copyDir(publicDir, destPublicDir);

console.log('Standalone build copied successfully!');
