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

// Copy the Next.js static export output
const outDir = path.join(__dirname, '..', 'packages', 'frontend', 'out');
const destDir = path.join(__dirname, '..', 'packages', 'app', 'out');

// Clean previous build
if (fs.existsSync(destDir)) {
  fs.rmSync(destDir, { recursive: true });
}

console.log('Copying static export...');
copyDir(outDir, destDir);

console.log('Frontend build copied successfully!');
