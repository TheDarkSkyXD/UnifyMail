const { spawn } = require('child_process');
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
