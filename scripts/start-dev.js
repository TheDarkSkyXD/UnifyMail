const { spawn, spawnSync } = require('child_process');
const path = require('path');
const electronPath = require('electron');

// ANSI colors for logging
const COLORS = {
    cyan: '\x1b[36m',
    green: '\x1b[32m',
    yellow: '\x1b[33m',
    red: '\x1b[31m',
    reset: '\x1b[0m'
};

function log(color, message) {
    console.log(`${color}[Start Script] ${message}${COLORS.reset}`);
}

log(COLORS.cyan, 'Initializing development environment...');

// 0. Build Rust addon (mailcore-rs) — skip if .node already exists and sources unchanged
//    Always rebuild in case Rust sources changed since last start.
log(COLORS.yellow, 'Building Rust addon (app/mailcore-rs)...');

// On Windows, dlltool.exe must be in PATH (from MSYS2 MinGW).
// LIBNODE_PATH must point to the directory containing libnode.dll.
// See app/mailcore-rs/README.md for setup instructions.
const napiPath = path.join(__dirname, '..', 'app', 'mailcore-rs', 'node_modules', '.bin',
    process.platform === 'win32' ? 'napi.cmd' : 'napi');
const mailcoreRsDir = path.join(__dirname, '..', 'app', 'mailcore-rs');

const buildEnv = {
    ...process.env,
    // MSYS2 MinGW dlltool required for Windows GNU target build
    PATH: process.platform === 'win32'
        ? `C:\\msys64\\mingw64\\bin;${process.env.PATH}`
        : process.env.PATH,
    // libnode.dll import library — generated from node.exe via gendef+dlltool
    LIBNODE_PATH: process.env.LIBNODE_PATH || (process.platform === 'win32' ? require('os').tmpdir() : undefined),
};

const buildArgs = ['build', '--platform', '--release'];
if (process.platform === 'win32') {
    buildArgs.push('--target', 'x86_64-pc-windows-gnu');
}

const rustBuild = spawnSync(`"${napiPath}"`, buildArgs, {
    shell: true,
    stdio: 'inherit',
    cwd: mailcoreRsDir,
    env: buildEnv,
});

if (rustBuild.status !== 0) {
    log(COLORS.red, `Rust addon build failed (exit code ${rustBuild.status}).`);
    log(COLORS.red, 'Ensure MSYS2 MinGW is installed and libnode.dll is at LIBNODE_PATH.');
    log(COLORS.red, 'See app/mailcore-rs/README.md for setup instructions.');
    process.exit(1);
}

log(COLORS.green, 'Rust addon built successfully.');

// 0b. Build Rust sync binary (mailsync-rs) — debug build only, incremental compilation
//     handles no-op builds in ~1-2s when sources are unchanged.
log(COLORS.yellow, 'Building Rust sync binary (app/mailsync-rs)...');

const mailsyncRsDir = path.join(__dirname, '..', 'app');
const mailsyncArgs = ['build', '-p', 'unifymail-sync'];
if (process.platform === 'win32') {
    mailsyncArgs.push('--target', 'x86_64-pc-windows-gnu');
}

const mailsyncBuild = spawnSync('cargo', mailsyncArgs, {
    shell: true,
    stdio: 'inherit',
    cwd: mailsyncRsDir,
    env: buildEnv,
});

if (mailsyncBuild.status !== 0) {
    log(COLORS.red, `Rust sync binary build failed (exit code ${mailsyncBuild.status}).`);
    log(COLORS.red, 'The C++ mailsync binary will be used as fallback.');
    // Non-fatal: C++ mailsync binary still functional during Phases 5-9
} else {
    log(COLORS.green, 'Rust sync binary built successfully.');
}

// 1. Start Tailwind CSS Watcher
log(COLORS.yellow, 'Starting Tailwind CSS watcher...');
const tailwind = spawn('npm', ['run', 'tailwind:dev', '--prefix', 'app'], {
    shell: true,
    stdio: 'inherit',
    env: { ...process.env, FORCE_COLOR: '1' }
});

tailwind.on('error', (err) => {
    log(COLORS.red, `Tailwind process error: ${err.message}`);
});

tailwind.on('close', (code) => {
    log(COLORS.yellow, `Tailwind watcher exited with code ${code}`);
});

// 2. Start Electron App
log(COLORS.green, `Starting Electron application from: ${electronPath}...`);

const electron = spawn(`"${electronPath}"`, ['./app', '--enable-logging', '--dev', '--remote-debugging-port=9222'], {
    shell: true,
    stdio: 'inherit',
    env: { ...process.env, FORCE_COLOR: '1' }
});

electron.on('error', (err) => {
    log(COLORS.red, `Electron process error: ${err.message}`);
});

// 3. Handle Cleanup
electron.on('close', (code) => {
    log(COLORS.cyan, `Electron exited with code ${code}. Cleaning up...`);
    cleanup(() => process.exit(code));
});

function cleanup(done) {
    try {
        if (process.platform === 'win32') {
            const kill = spawn('taskkill', ['/pid', String(tailwind.pid), '/f', '/t'], { stdio: 'ignore' });
            kill.on('close', () => done && done());
            kill.on('error', () => done && done());
        } else {
            try { process.kill(-tailwind.pid); } catch (e) { /* already dead */ }
            if (done) done();
        }
    } catch (e) {
        log(COLORS.red, `Cleanup error: ${e.message}`);
        if (done) done();
    }
}

// Handle script termination
process.on('SIGINT', () => {
    log(COLORS.red, 'Script terminated (SIGINT). Cleaning up...');

    if (process.platform === 'win32') {
        const killElectron = spawn('taskkill', ['/pid', String(electron.pid), '/f', '/t'], { stdio: 'ignore' });
        killElectron.on('close', () => cleanup(() => process.exit()));
        killElectron.on('error', () => cleanup(() => process.exit()));
    } else {
        try { electron.kill(); } catch (e) { }
        cleanup(() => process.exit());
    }
});
