const fs = require('fs');
const path = require('path');
const crypto = require('crypto');
const yaml = require('js-yaml');

// Path to the 'app' directory
const appDir = path.resolve(__dirname, '..');
const distDir = path.join(appDir, 'dist');
const packageJson = require(path.join(appDir, 'package.json'));

const version = packageJson.version;
// Assuming artifact is named UnifyMail-AppleSilicon.zip or UnifyMail.zip
const files = fs.readdirSync(distDir).filter(f => f.endsWith('.zip') && f.includes('UnifyMail'));

if (files.length === 0) {
    console.error('No zip artifact found in dist directory.');
    process.exit(1);
}

const zipFile = files[0]; // Pick the first zip found
const filePath = path.join(distDir, zipFile);
const fileBuffer = fs.readFileSync(filePath);
const sha512 = crypto.createHash('sha512').update(fileBuffer).digest('base64');
const releaseDate = new Date().toISOString();

const yamlData = {
    version: version,
    files: [
        {
            url: zipFile,
            sha512: sha512,
            size: fileBuffer.length
        }
    ],
    path: zipFile,
    sha512: sha512,
    releaseDate: releaseDate
};

const yamlStr = yaml.dump(yamlData, { lineWidth: -1 });

// Output filename based on architecture
// electron-updater will check latest-mac.yml. If we are building for ARM64, we might want to produce latest-mac-arm64.yml??
// No, typically there is just one latest-mac.yml which lists multiple files.
// Since we are running independent builds, they will overwrite each other.
// For now, we output latest-mac.yml. This is a known limitation of splitting builds without a merge step.
const outputFilename = 'latest-mac.yml';

fs.writeFileSync(path.join(distDir, outputFilename), yamlStr);
console.log(`Generated ${outputFilename} for ${zipFile}`);
