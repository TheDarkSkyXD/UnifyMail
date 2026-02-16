const https = require('https');
const fs = require('fs');
const path = require('path');
const { execSync } = require('child_process');
const targz = require('targz');

const appPath = path.resolve(__dirname, '..', 'app');
const mailsyncPath = path.resolve(__dirname, '..', 'mailsync');

function getMailsyncURL() {
    const distKey = `${process.platform}-${process.arch}`;
    const distDir = {
        'darwin-x64': 'osx',
        'darwin-arm64': 'osx',
        'win32-x64': 'win-ia32', // Mailsync is 32-bit on Windows
        'win32-ia32': 'win-ia32',
        'linux-x64': 'linux',
        'linux-arm64': 'linux-arm64',
        'linux-ia32': null,
    }[distKey];

    if (!distDir) {
        throw new Error(`Platform ${distKey} is not supported.`);
    }

    console.log(`Checking mailsync submodule status...`);
    let hash;
    /*
    try {
        const out = execSync('git submodule status ./mailsync', { cwd: path.join(__dirname, '..') });
        const match = /[\+-]([A-Za-z0-9]{8})/.exec(out.toString());
        if (!match) throw new Error('Could not parse submodule hash');
        hash = match[1];
    } catch (e) {
        console.warn("Could not get submodule hash, defaulting to a known stable hash (e060ec0a)...");
        hash = 'e060ec0a';
    }
    */
    hash = 'e060ec0a'; // Force known stable hash


    return `https://mailspring-builds.s3.amazonaws.com/mailsync/${hash}/${distDir}/mailsync.tar.gz`;
}

function downloadAndExtract() {
    const url = getMailsyncURL();
    const dest = path.join(appPath, 'mailsync.tar.gz');

    console.log(`Downloading mailsync from: ${url}`);

    const file = fs.createWriteStream(dest);
    https.get(url, (response) => {
        if (response.statusCode !== 200) {
            console.error(`Failed to download: HTTP ${response.statusCode}`);
            if (response.statusCode === 403 || response.statusCode === 404) {
                console.error("The specific commit hash might not have a pre-built binary. Try updating the submodule.");
            }
            return;
        }

        response.pipe(file);

        file.on('finish', () => {
            file.close(() => {
                console.log('Download completed. Extracting...');

                targz.decompress({
                    src: dest,
                    dest: appPath
                }, (err) => {
                    if (err) {
                        console.error('Extraction failed:', err);
                    } else {
                        console.log('Successfully installed mailsync binary to ./app/');
                        // Cleanup tarball
                        try { fs.unlinkSync(dest); } catch (e) { }
                    }
                });
            });
        });
    }).on('error', (err) => {
        fs.unlink(dest, () => { });
        console.error('Download error:', err.message);
    });
}

downloadAndExtract();
